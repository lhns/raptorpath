#!/bin/bash
# GOAL "HONEST INPUTS" phase 2 — THE SCORED BATTERY.
# Contract: goal-gate "Honest Inputs — PRE-REGISTRATION" (commit 6f6f2a9).
# That block is scored against, never modified.
#
#   sudo bash hi_battery.sh <seed> [reps]
#
# FIVE ARMS, same-session interleaved per cell (discipline 3):
#
#   A   (unset)                          shipped control
#   D   RWM_PLAIN_RS=1                   the measured defect, reproduced
#   DH  RWM_PLAIN_RS=1 RWM_HONEST_ANCHOR=1
#                                        fix 1 isolated on the honest sampler
#   B   RWM_THREE_TERM=1 RWM_PLAIN_RS=1  the three-term composed arm, reproduced
#   BH  B + RWM_HONEST_ANCHOR=1 RWM_HONEST_K=1
#                                        the law on honest, affordable inputs
#
# CELLS (the pre-registration's headroom table governs what each may claim):
#   c1    topo c1/c1 single 400 MB   H1/H2: DH/A parity + CPU/byte gauge
#   jit25 adv  50 MB                 H3: the [3T] limit back inside 1300-1430
#   sc2   topo c2/c2 single 100 MB   H4: the latency win survives at parity
#   c7    topo c2/c2 dual 200 MB     H5: DH > D (is D/A 0.88 pure CPU?)
#   c8    topo c2/c3 dual 25 MB      H5: no regression; abort class, n reported
#
# INSTRUMENTS on every invocation: the `CPU: CPUSRV=/CPUCLI=` gauge (H2 is
# scored on it — an invocation without it is INSTRUMENT-FAIL, loud), RWM_DIAG=1
# (khr=/kraw= decomposes the K bias in-cell at jit25), tc -s qdisc capture
# (discipline 16), and RWM_LATPROBE=1 on the topo cells (H4's independent ICMP
# flow; jit25 carries no latency claim and adv_cells has no probe rig).
#
# LIVENESS, asserted per arm and per direction BEFORE any number is read
# (discipline 1/15): all four gates two-sided on the `[GATES]` line of BOTH
# endpoints; the `O(1) windowed-max rate filter ACTIVE` echo PRESENT on DH/BH
# and ABSENT elsewhere; the `raw-sample echo-ratio floor ACTIVE` echo PRESENT
# on BH and ABSENT elsewhere; `[3T] eng=1` on B/BH. An arm whose echo set is
# wrong is VOID and re-run, not explained.
#
# ABORT ≠ DNF ≠ INSTRUMENT-FAIL, as encoded in hi_parse.py (no summary at all
# = ABORT; the seed-7 topo-ping abort class is handled by symmetric re-runs
# only, never asymmetric top-ups).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh   # rwm_forward_env for the adv (jit25) direct launches
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
HI_CELLS="${RWM_HI_CELLS:-c1 jit25 sc2 c7 c8}"
HI_ARMS="${RWM_HI_ARMS:-A D DH B BH}"
TAG="${RWM_HI_TAG:-hi}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/honestinputs/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/honestinputs/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

arm_env() { case "$1" in
  A)  echo "" ;;
  D)  echo "RWM_PLAIN_RS=1" ;;
  DH) echo "RWM_PLAIN_RS=1 RWM_HONEST_ANCHOR=1" ;;
  B)  echo "RWM_THREE_TERM=1 RWM_PLAIN_RS=1" ;;
  BH) echo "RWM_THREE_TERM=1 RWM_PLAIN_RS=1 RWM_HONEST_ANCHOR=1 RWM_HONEST_K=1" ;;
esac; }
arm_3t() { case "$1" in B|BH) echo 1 ;; *) echo 0 ;; esac; }
arm_rs() { case "$1" in A) echo 0 ;; *) echo 1 ;; esac; }
arm_ha() { case "$1" in DH|BH) echo 1 ;; *) echo 0 ;; esac; }
arm_hk() { case "$1" in BH) echo 1 ;; *) echo 0 ;; esac; }

# topo cells -> "scenA scenB mode bytes"; jit25 handled by run_adv.
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual 200000000" ;;
    c8)  echo "c2 c3 dual 25000000" ;;
    *) echo "" ;;
  esac
}

# ── LIVENESS + PARSE, shared by both substrates ──────────────────────────
# args: name cpusrv cpucli ping_path q_path  (logs at /tmp/rwm-{c,s}.log)
check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local e3t ers eha ehk
  e3t="$(arm_3t "$arm")"; ers="$(arm_rs "$arm")"
  eha="$(arm_ha "$arm")"; ehk="$(arm_hk "$arm")"

  python3 ./hi_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "HIRESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line (the amendment-1 lesson: the ACTIVE echoes'
  # own prose contains literal `RWM_*=0` strings).
  local g3c g3s grc grs ghc ghs gkc gks a3 hac has hkc hks eng
  g3c=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  g3s=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  grc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  grs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  ghc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_ANCHOR=[01]")
  ghs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_ANCHOR=[01]")
  gkc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_K=[01]")
  gks=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_K=[01]")
  a3=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  hac=$(grep -c "windowed-max rate filter ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  has=$(grep -c "windowed-max rate filter ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  hkc=$(grep -c "echo-ratio floor ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  hks=$(grep -c "echo-ratio floor ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  eng=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep '\[3T\]' | grep -c "eng=1" || true)
  echo "LIVENESS $name rep=$REP cli=[$g3c $grc $ghc $gkc] srv=[$g3s $grs $ghs $gks] act3=$a3 actHA=$hac/$has actHK=$hkc/$hks eng1=$eng (expect 3t=$e3t rs=$ers ha=$eha hk=$ehk)" >> "$OUT"

  # An invocation with no [GATES] on either endpoint is an ABORT: no datum,
  # no liveness verdict (hi_parse.py records it; the report excludes it).
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
  # The fix gates' ACTIVE echoes: PRESENT on their arms, ABSENT elsewhere.
  if [ "$eha" = "1" ]; then
    { [ "$hac" -eq 0 ] || [ "$has" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL-HA-ECHO $name rep=$REP (VOID: cli=$hac srv=$has)" >> "$OUT"
  else
    { [ "$hac" -gt 0 ] || [ "$has" -gt 0 ]; } && echo "ARM-CONTAMINATION-HA $name rep=$REP" >> "$OUT"
  fi
  if [ "$ehk" = "1" ]; then
    { [ "$hkc" -eq 0 ] || [ "$hks" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL-HK-ECHO $name rep=$REP (VOID: cli=$hkc srv=$hks)" >> "$OUT"
  else
    { [ "$hkc" -gt 0 ] || [ "$hks" -gt 0 ]; } && echo "ARM-CONTAMINATION-HK $name rep=$REP" >> "$OUT"
  fi
  if [ "$e3t" = "1" ]; then
    [ "$a3" -eq 0 ] && echo "ARM-LIVENESS-FAIL-ACTIVE $name rep=$REP" >> "$OUT"
    [ "$eng" -eq 0 ] && echo "ARM-LIVENESS-FAIL-3T $name rep=$REP (VOID: no eng=1)" >> "$OUT"
  else
    [ "$a3" -gt 0 ] && echo "ARM-CONTAMINATION-ACTIVE $name rep=$REP" >> "$OUT"
    [ "$eng" -gt 0 ] && echo "ARM-CONTAMINATION-3T $name rep=$REP" >> "$OUT"
  fi
  # H2's gauge is the mechanism's conviction gauge: absent = INSTRUMENT-FAIL.
  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP" >> "$OUT"

  # The [3T] readouts verbatim (first + last engaged line) on the law arms.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[3T\]' | grep 'eng=1' \
    | grep -oE "eng=[01] cap=[0-9]+ window=[0-9.]+ slack=[0-9.]+ span=[0-9.]+ rho=[0-9.]+ b=[0-9.]+" \
    | sed -n '1p;$p' | sed "s/^/3TLINE $name rep=$REP /" >> "$OUT") || true

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

  # Probe liveness (topo cells only — the cells carrying latency claims).
  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_adv() { # jit25 arm — adv_cells substrate, tt_adv.sh conventions + CPU gauge
  local cell="$1" arm="$2" name="$1-$2"
  local envs; envs="$(arm_env "$arm")"
  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=50000000 $(date -u +%T)" >> "$OUT"
  pkill -x raptorpath 2>/dev/null || true
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-q.txt /tmp/rwm-cli-time

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
      --window-reliable --protocol-hint bulk >/tmp/rwm-s.log 2>&1 &
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
  # CPU gauge, both roles (H2's instrument, perf_rwm_c.sh's exact method):
  # client under /usr/bin/time -v; server ticks off /proc before teardown.
  # shellcheck disable=SC2086
  timeout 400 ip netns exec rp-cli /usr/bin/time -v -o /tmp/rwm-cli-time \
      env $(rwm_forward_env) RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=180 $envs \
      "$BIN" perf --client --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
      --window-reliable --protocol-hint bulk \
      --bytes 50000000 --runs 1 >/tmp/rwm-c.log 2>&1
  local rc=$?
  [ "$rc" -ne 0 ] && echo "CLIENT-RC $name rep=$REP rc=$rc" >> "$OUT"
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  local SRV_TICKS=0 T P HZ cpus cpuc CLI_U CLI_S
  for P in $(pgrep -x raptorpath); do
    T=$(awk '{print $14+$15}' /proc/$P/stat 2>/dev/null || echo 0)
    SRV_TICKS=$((SRV_TICKS + T))
  done
  HZ=$(getconf CLK_TCK)
  CLI_U=$(grep -oP 'User time \(seconds\): \K[0-9.]+' /tmp/rwm-cli-time 2>/dev/null || echo 0)
  CLI_S=$(grep -oP 'System time \(seconds\): \K[0-9.]+' /tmp/rwm-cli-time 2>/dev/null || echo 0)
  cpus=$(awk "BEGIN{printf \"%.2f\", $SRV_TICKS/$HZ}")
  cpuc=$(awk "BEGIN{printf \"%.2f\", $CLI_U+$CLI_S}")
  echo "    CPU: CPUSRV=${cpus}s CPUCLI=${cpuc}s (srv=decoder cli=sender; whole-invocation incl warmup)" >> "$OUT"

  # tc counters BEFORE teardown (discipline 16), + wall secs so utilisation
  # is computable from the capture alone (perf_rwm_c.sh's convention).
  { bash ./adv_cells.sh counters
    echo "== INVOCATION_S $(( $(date +%s) - t0 ))"
  } > /tmp/rwm-q.txt 2>/dev/null || true

  check_and_parse "$name" "$cell" "$arm" "$cpus" "$cpuc" "" /tmp/rwm-q.txt
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"

  pkill -x raptorpath 2>/dev/null || true
  bash ./adv_cells.sh down >/dev/null 2>&1 || true
}

run_one() { # cell arm
  case " $HI_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $HI_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  if [ "$1" = "jit25" ]; then run_adv "$1" "$2"; else run_topo "$1" "$2"; fi
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

{
  echo "=== HI BATTERY seed=$SEED_ARG reps=$REPS cells='$HI_CELLS' arms='$HI_ARMS' $(date -u +%FT%TZ)"
  echo "=== binary sha256 $(sha256sum $BIN | cut -d' ' -f1)"
  echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

for REP in $(seq 1 "$REPS"); do
  for CELL in $HI_CELLS; do
    for ARM in $HI_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# Per-arm tally: an arm that VANISHED must fail loudly (discipline 7).
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $HI_CELLS; do
  for A in $HI_ARMS; do
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A n=$N/$REPS" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "HI-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo HI-BATTERY-DONE-$SEED_ARG
