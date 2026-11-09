#!/bin/bash
# THE LOOP-TIME DIAGNOSTIC (docs/status.md §23 follow-up; paper §6.1 "The law
# is fed by its own output"). Derived from anchordiag_battery.sh. ONE binary
# (main + the RWM_LOOPDIAG instrument, built fresh).
#
#   sudo env LD_BIN=<bin> LD_SHA=<sha256> LD_OUTDIR=<dir> LD_TAG=<tag> \
#     [LD_SMOKE=1] bash loopdiag_battery.sh
#
# Per path and per 250 ms [DIAG] window, [LOOPDIAG] prints the echo-RTT loop
# time (min/p10/p50), Copa's RTprop/jitter/srtt, the delivered rate and quinn's
# own BBR pacing rate side by side.
# Arms: MAIN (no arm env), S<n> (RWM_STORE=<n>).
# PLAN (46 invocations; the smoke is 2), blocks (seed,rep) = (42,1) (7,1) (42,2):
#   per block: c1s-400 MAIN, c2-100 MAIN; then the c1s-400 store sweep
#   n in {192,256,384,512,768,1024,1536,2048} (ascending on even blocks,
#   descending on odd); then, blocks 0 and 1 only, the c2-100 sweep
#   n in {64,96,128,192,256,384,512,1024} (same direction rule).
#   3 x (2 + 8) + 2 x 8 = 46.
# PLAN 27 (LD_PLAN=27, status section 27; plan 24 above is the default and unchanged):
#   arms DEF<n> = shipped defaults + RWM_STORE=<n>; BRS<n> = RWM_QUIC_CC=bbr_rs
#   (the in-tree burst-robust BBR, src/transport/bbr_rs.rs) + RWM_STORE=<n>.
#   Two blocks (seed,rep) = (42,1) (7,1); per block, cells c2-100, c1s-400, c8-100,
#   c1d-400 in that order; per cell the DEF sweep and the BRS sweep (DEF first on
#   even blocks, BRS first on odd), each ascending on even blocks, descending on odd.
#   grids: c1s 192..2048, c2 64..1024, c1d 384..4096, c8 128..2048 (8 each).
#   2 x 4 x 2 x 8 = 128 invocations; the smoke is 2 (c2-100 BRS192, c8-100 DEF512).
# PLAN 29 (LD_PLAN=29, the power store law; plans 24 and 27 unchanged):
#   arms MAIN (no arm env), PWR (RWM_STORE_LAW=power), KNEE (RWM_STORE=<the cell's knee>:
#   c2-100 192, c1s-400 1024, c1d-400 768, c8-100 384). Three blocks (seed,rep) = (42,1) (7,1) (42,2);
#   per block the cells c2-100, c1s-400, c8-100, c1d-400, each running the three arms in an order
#   rotated by block (MAIN,PWR,KNEE / PWR,KNEE,MAIN / KNEE,MAIN,PWR). 3 x 4 x 3 = 36 invocations;
#   the smoke is 2 (c2-100 PWR s42, c1s-400 PWR s42). The plan table lives in loopdiag_parse.py
#   (`plan29 [smoke]`, one `cell arm seed rep knee` line per row); this script only executes it.
# PLAN 34 (LD_PLAN=34, status section 34; earlier plans unchanged): arms DEF<n> only
#   (shipped substrate BBR + RWM_STORE=<n>), RWM_RDIAG=1 added on every row. Cells c1s-400
#   {256..2048} and c1d-400 {384..2048}; blocks (42,1) (7,1) (42,2), c1s first and ascending on
#   even blocks, c1d first and descending on odd. 39 invocations; the smoke is 2 (c1s-400 DEF384,
#   c1d-400 DEF768). The table is loopdiag_parse.py `plan34 [smoke]`.
# PLAN 36 (LD_PLAN=36, status section 36 + amendments 1-2; earlier plans unchanged): main's binary, which has no
#   [LOOPDIAG]/[ACKDIAG], so neither is requested or required. Arms CAP<n> (RWM_STORE=<n> + a tcpdump per data leg,
#   RWM_PCAP_DIR) and NOCAP<n> (RWM_STORE=<n>, no capture). RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=300. Pcaps go to
#   $LD_OUTDIR/pcap/<row>/ (tmpfs); after each row loopdiag_parse.py (-> disp36.py) scores them, writes the kept per-skb
#   tables to $LD_OUTDIR/tables/, and the pcaps are deleted. 56 invocations; the smoke is 2 (c1s-400 CAP256,
#   c8-100 CAP128). The table is `loopdiag_parse.py plan36 [smoke]` (disp36.plan36).
# Launched by loopdiag_run_all.sh (the envelope, which holds both VM locks).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh loopdiag_battery.sh loopdiag_parse.py anchordiag_parse.py stage3_parse.py \
    storelaw_parse.py disp36.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py

BIN="${LD_BIN:?LD_BIN}"
WANT="${LD_SHA:?LD_SHA}"
OUTDIR="${LD_OUTDIR:?LD_OUTDIR}"
TAG="${LD_TAG:-ld}"
TRIES="${LD_BRINGUP_TRIES:-2}"
PLAN="${LD_PLAN:-24}"
case "$PLAN" in 24|27|29|34|36) ;; *) echo "ABORT-PLAN LD_PLAN=$PLAN (24|27|29|34|36)"; exit 3 ;; esac
# Plan 34 reads the receiver task's [RDIAG] line (R3); the other plans keep it unset.
RDIAG_ENV=()
[ "$PLAN" = "34" ] && RDIAG_ENV=(RWM_RDIAG=1)
# The per-plan instrument env and the driver-output lines the ledger keeps.
INSTR_ENV=(RWM_ACKDIAG=1 RWM_LOOPDIAG=1 RWM_PERF_TIMEOUT_S=150)
KEEP_PAT='summary|"dnf"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf'
if [ "$PLAN" = "36" ]; then
  INSTR_ENV=(RWM_PERF_TIMEOUT_S=300)
  KEEP_PAT="$KEEP_PAT|\[CAPTURE\]|QDISC cli"
fi
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=loopdiag_battery
LB_LOG="$OUT"

# The instruments and gates the arms need must be in the binary.
# Plan 36 runs main's binary, which lacks both instruments: it asks only for the store override it sets.
if [ "$PLAN" = "36" ]; then preflight_binary "$BIN" "RWM_STORE"; else preflight_binary "$BIN" "ACKDIAG" "LOOPDIAG"; fi
[ "$PLAN" = "27" ] && preflight_binary "$BIN" "burst-robust BBR"
[ "$PLAN" = "29" ] && preflight_binary "$BIN" "power store law ACTIVE"
GOT="$(sha256sum "$BIN" | cut -d' ' -f1)"
[ "$GOT" = "$WANT" ] || { _lb_say "ABORT-SHA start: $GOT != $WANT"; exit 5; }
if pgrep -x raptorpath >/dev/null 2>&1; then
  _lb_say "BUSY: raptorpath already running -- aborting"; exit 3
fi

cell_spec() { # cell -> "scenA scenB mode bytes"
  case "$1" in
    c1s-400) echo "c1 c1 single 400000000" ;;
    c2-100)  echo "c2 c2 single 100000000" ;;
    c1d-400) echo "c1 c1 dual 400000000" ;;
    c8-100)  echo "c2 c3 dual 100000000" ;;
    *) echo "" ;;
  esac
}

# /proc/pressure/io `full avg10`: a per-row covariate (flagged in the parser, never a void).
io_full10() { awk '/^full/{for(i=1;i<=NF;i++) if($i ~ /^avg10=/){sub("avg10=","",$i); print $i}}' /proc/pressure/io 2>/dev/null || echo -1; }

run_one() { # cell arm seed rep [knee]    (arm: MAIN|S<n>|DEF<n>|BRS<n>|PWR|KNEE; knee = the cell's knee n, KNEE only)
  local cell="$1" arm="$2" seed="$3" rep="$4" knee="${5:-}" ca cb mode bytes t0 rc wall attempt sha
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  local name="$cell-$arm"
  local arm_env=()
  case "$arm" in
    MAIN) ;;
    S[0-9]*) arm_env=(RWM_STORE="${arm#S}") ;;
    DEF[0-9]*) arm_env=(RWM_STORE="${arm#DEF}") ;;
    BRS[0-9]*) arm_env=(RWM_QUIC_CC=bbr_rs RWM_STORE="${arm#BRS}") ;;
    CAP[0-9]*|NOCAP[0-9]*)
          [ "$PLAN" = "36" ] || { echo "UNKNOWN-ARM $arm (plan 36 only)" >> "$OUT"; return 0; }
          arm_env=(RWM_STORE="${arm##*[A-Z]}") ;;
    PWR) arm_env=(RWM_STORE_LAW=power) ;;
    KNEE) [[ "$knee" =~ ^[0-9]+$ ]] || { echo "UNKNOWN-KNEE $cell '$knee'" >> "$OUT"; return 0; }
          arm_env=(RWM_STORE="$knee") ;;
    *) echo "UNKNOWN-ARM $arm" >> "$OUT"; return 0 ;;
  esac
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$BIN" | cut -d' ' -f1)"
    if [ "$sha" != "$WANT" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: $sha != $WANT -- battery stopped"; exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=bulk spec=$ca/$cb/$mode bytes=$bytes arm_env='${arm_env[*]}' bin=$BIN sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/ld-drv.out
    # Plan 36: the CAP arm's pcap dir on tmpfs (fresh per attempt); NOCAP and the other plans set no RWM_PCAP_DIR.
    pdir="$OUTDIR/pcap/${name}-s${seed}-r${rep}-a${attempt}"
    cap_env=()
    if [ "$PLAN" = "36" ] && [[ "$arm" == CAP* ]]; then rm -rf "$pdir"; mkdir -p "$pdir"; cap_env=(RWM_PCAP_DIR="$pdir"); fi
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    io_b=$(io_full10)
    t0=$(date +%s)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_ACKDIAG -u RWM_LOOPDIAG -u RWM_IO_RT -u RWM_STORE_PATHS \
        -u RWM_PLAIN_RS -u RWM_HONEST_CAP -u RWM_STORE -u RWM_QUIC_CC -u RWM_STORE_LAW -u RWM_RDIAG -u RWM_PCAP_DIR \
        "${arm_env[@]}" "${RDIAG_ENV[@]}" "${cap_env[@]}" \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 "${INSTR_ENV[@]}" \
        RWM_C_PIPELINE=window RWM_BIN="$BIN" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/ld-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    io_a=$(io_full10)
    io_max=$(awk -v a="$io_b" -v b="$io_a" 'BEGIN{print (a>b)?a:b}')
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    echo "IOPRESSURE $name s$seed rep=$rep full_avg10_before=$io_b full_avg10_after=$io_a $(grep '^full' /proc/pressure/io)" >> "$OUT"
    grep -aE "$KEEP_PAT" /tmp/ld-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/ld-drv.out "$base-drv.out" 2>/dev/null || true
    cp /tmp/rwm-c.log "$base-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$base-s.log" 2>/dev/null || true
    if [ "$rc" = "0" ] && ! grep -aq '"summary"' /tmp/rwm-c.log 2>/dev/null \
        && [ "$attempt" -lt "$TRIES" ]; then
      echo "RUN-RETRY $name s$seed rep=$rep attempt=$attempt (no summary)" >> "$OUT"
      continue
    fi
    break
  done
  if [ "$rc" = "0" ] && ! grep -aq '"summary"' /tmp/rwm-c.log 2>/dev/null; then
    echo "ABORT-BRINGUP $name s$seed rep=$rep after $TRIES attempts: NO_DATA" >> "$OUT"
  fi
  [ $(( cot_b + cot_a )) -gt 0 ] && cot=1
  ondisk=0
  case "$BIN" in /dev/shm/*) ;; *) ondisk=1 ;; esac
  case "$OUTDIR" in /dev/shm/*) ;; *) ondisk=1 ;; esac
  if LD_PLAN="$PLAN" LD_ONDISK="$ondisk" LD_IO_FULL10="$io_max" LD_PCAP_DIR="$pdir" LD_TABLE_DIR="$OUTDIR/tables" \
      python3 ./loopdiag_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/ld-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1; then
    # Scored (summary in the ledger, per-skb tables kept): only now do the pcaps go.
    [ "$PLAN" = "36" ] && rm -rf "$OUTDIR/pcap/${name}-s${seed}-r${rep}-"a*
  else
    echo "LDROW-PARSE-FAIL $name s$seed rep=$rep (pcaps kept under $OUTDIR/pcap)" >> "$OUT"
  fi
}

echo "=== LOOP-DIAG BATTERY tag=$TAG smoke=${LD_SMOKE:-0} $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary $BIN sha256 $WANT" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 ${INSTR_ENV[*]} bulk window runs=1; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_LOOPDIAG RWM_IO_RT RWM_STORE_PATHS RWM_PLAIN_RS RWM_HONEST_CAP RWM_STORE RWM_QUIC_CC RWM_STORE_LAW RWM_RDIAG RWM_PCAP_DIR; plan=$PLAN (plan 36: no ACKDIAG/LOOPDIAG; CAP<n> sets RWM_PCAP_DIR=<pcap dir>) (plan 34 adds RWM_RDIAG=1); S<n>/DEF<n> set RWM_STORE=<n>, BRS<n> sets RWM_QUIC_CC=bbr_rs RWM_STORE=<n>, PWR sets RWM_STORE_LAW=power, KNEE sets RWM_STORE=<cell knee>" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

# ── plan 27 ───────────────────────────────────────────────────────────────
plan27() { # smoke -> the whole plan (0), or the two smoke rows (1)
  local smoke="$1" b seed rep cell fam n i order first second spec
  local -a grid
  declare -A GRID27=([c1s-400]="192 256 384 512 768 1024 1536 2048" [c2-100]="64 96 128 192 256 384 512 1024"
                     [c1d-400]="384 512 768 1024 1536 2048 3072 4096" [c8-100]="128 192 256 384 512 768 1024 2048")
  if [ "$smoke" = "1" ]; then
    run_one c2-100 BRS192 42 1
    run_one c8-100 DEF512 42 1
    return 0
  fi
  b=0
  for spec in "42 1" "7 1"; do
    read -r seed rep <<< "$spec"
    for cell in c2-100 c1s-400 c8-100 c1d-400; do
      read -r -a grid <<< "${GRID27[$cell]}"
      if [ $(( b % 2 )) -eq 0 ]; then order=("${grid[@]}"); first=DEF; second=BRS
      else
        order=(); for (( i=${#grid[@]}-1; i>=0; i-- )); do order+=("${grid[$i]}"); done
        first=BRS; second=DEF
      fi
      for fam in "$first" "$second"; do
        for n in "${order[@]}"; do run_one "$cell" "$fam$n" "$seed" "$rep"; done
      done
    done
    echo "BLOCK-COMPLETE rep=$rep seed=$seed block=$b $(date -u +%FT%TZ)" >> "$OUT"
    b=$(( b + 1 ))
  done
  BLOCK=$b
}

# ── plan 29 ───────────────────────────────────────────────────────────────
# The plan table is loopdiag_parse.py `plan29` (read into an array first, so no
# command inside run_one can eat the table from stdin).
plan29() { # smoke -> the whole plan (0), or the two smoke rows (1)
  local -a tbl
  local line cell arm seed rep knee prev="" b=0 last_seed="" last_rep=""
  if [ "$1" = "1" ]; then
    mapfile -t tbl < <(python3 ./loopdiag_parse.py plan29 smoke)
    [ "${#tbl[@]}" -eq 2 ] || { echo "ABORT-PLAN29 smoke table has ${#tbl[@]} rows" >> "$OUT"; exit 3; }
  else
    mapfile -t tbl < <(python3 ./loopdiag_parse.py plan29)
    [ "${#tbl[@]}" -eq 36 ] || { echo "ABORT-PLAN29 table has ${#tbl[@]} rows (36)" >> "$OUT"; exit 3; }
  fi
  for line in "${tbl[@]}"; do
    read -r cell arm seed rep knee <<< "$line"
    if [ "$1" != "1" ] && [ -n "$prev" ] && [ "$prev" != "$seed/$rep" ]; then
      echo "BLOCK-COMPLETE rep=$last_rep seed=$last_seed block=$b $(date -u +%FT%TZ)" >> "$OUT"
      b=$(( b + 1 ))
    fi
    prev="$seed/$rep"; last_seed="$seed"; last_rep="$rep"
    run_one "$cell" "$arm" "$seed" "$rep" "$knee"
  done
  if [ "$1" != "1" ]; then
    echo "BLOCK-COMPLETE rep=$last_rep seed=$last_seed block=$b $(date -u +%FT%TZ)" >> "$OUT"
    BLOCK=$(( b + 1 ))
  fi
}

# ── plan 34 ───────────────────────────────────────────────────────────────
plan34() { # smoke -> the whole plan (0), or the two smoke rows (1)
  local -a tbl
  local line cell arm seed rep prev="" b=0 last_seed="" last_rep=""
  if [ "$1" = "1" ]; then
    mapfile -t tbl < <(python3 ./loopdiag_parse.py plan34 smoke)
    [ "${#tbl[@]}" -eq 2 ] || { echo "ABORT-PLAN34 smoke table has ${#tbl[@]} rows" >> "$OUT"; exit 3; }
  else
    mapfile -t tbl < <(python3 ./loopdiag_parse.py plan34)
    [ "${#tbl[@]}" -eq 39 ] || { echo "ABORT-PLAN34 table has ${#tbl[@]} rows (39)" >> "$OUT"; exit 3; }
  fi
  for line in "${tbl[@]}"; do
    read -r cell arm seed rep <<< "$line"
    if [ "$1" != "1" ] && [ -n "$prev" ] && [ "$prev" != "$seed/$rep" ]; then
      echo "BLOCK-COMPLETE rep=$last_rep seed=$last_seed block=$b $(date -u +%FT%TZ)" >> "$OUT"
      b=$(( b + 1 ))
    fi
    prev="$seed/$rep"; last_seed="$seed"; last_rep="$rep"
    run_one "$cell" "$arm" "$seed" "$rep"
  done
  if [ "$1" != "1" ]; then
    echo "BLOCK-COMPLETE rep=$last_rep seed=$last_seed block=$b $(date -u +%FT%TZ)" >> "$OUT"
    BLOCK=$(( b + 1 ))
  fi
}

# ── plan 36 ───────────────────────────────────────────────────────────────
plan36() { # smoke -> the whole plan (0), or the two smoke rows (1)
  local -a tbl
  local line cell arm seed rep prev="" b=0 last_seed="" last_rep=""
  if [ "$1" = "1" ]; then
    mapfile -t tbl < <(python3 ./loopdiag_parse.py plan36 smoke)
    [ "${#tbl[@]}" -eq 2 ] || { echo "ABORT-PLAN36 smoke table has ${#tbl[@]} rows" >> "$OUT"; exit 3; }
  else
    mapfile -t tbl < <(python3 ./loopdiag_parse.py plan36)
    [ "${#tbl[@]}" -eq 56 ] || { echo "ABORT-PLAN36 table has ${#tbl[@]} rows (56)" >> "$OUT"; exit 3; }
  fi
  for line in "${tbl[@]}"; do
    read -r cell arm seed rep <<< "$line"
    if [ "$1" != "1" ] && [ -n "$prev" ] && [ "$prev" != "$seed/$rep" ]; then
      echo "BLOCK-COMPLETE rep=$last_rep seed=$last_seed block=$b $(date -u +%FT%TZ)" >> "$OUT"
      b=$(( b + 1 ))
    fi
    prev="$seed/$rep"; last_seed="$seed"; last_rep="$rep"
    run_one "$cell" "$arm" "$seed" "$rep"
  done
  if [ "$1" != "1" ]; then
    echo "BLOCK-COMPLETE rep=$last_rep seed=$last_seed block=$b $(date -u +%FT%TZ)" >> "$OUT"
    BLOCK=$(( b + 1 ))
  fi
}

if [ "$PLAN" = "36" ]; then
  if [ "${LD_SMOKE:-0}" = "1" ]; then
    plan36 1
    echo "LD-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
    exit 0
  fi
  plan36 0
  echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
  grep -ac '^LDROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
  echo "LD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi

if [ "$PLAN" = "34" ]; then
  if [ "${LD_SMOKE:-0}" = "1" ]; then
    plan34 1
    echo "LD-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
    exit 0
  fi
  plan34 0
  echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
  grep -ac '^LDROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
  echo "LD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi

if [ "$PLAN" = "29" ]; then
  if [ "${LD_SMOKE:-0}" = "1" ]; then
    plan29 1
    echo "LD-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
    exit 0
  fi
  plan29 0
  echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
  grep -ac '^LDROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
  echo "LD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi

if [ "$PLAN" = "27" ]; then
  if [ "${LD_SMOKE:-0}" = "1" ]; then
    plan27 1
    echo "LD-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
    exit 0
  fi
  plan27 0
  echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
  grep -ac '^LDROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
  echo "LD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi

if [ "${LD_SMOKE:-0}" = "1" ]; then
  run_one c1s-400 MAIN 42 1
  run_one c2-100 S192 42 1
  echo "LD-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi

GRID_C1S=(192 256 384 512 768 1024 1536 2048)
GRID_C2=(64 96 128 192 256 384 512 1024)
sweep() { # cell block seed rep n...   (ascending on even blocks, descending on odd)
  local cell="$1" b="$2" seed="$3" rep="$4" n i
  shift 4
  local grid=("$@")
  if [ $(( b % 2 )) -eq 0 ]; then
    for n in "${grid[@]}"; do run_one "$cell" "S$n" "$seed" "$rep"; done
  else
    for (( i=${#grid[@]}-1; i>=0; i-- )); do run_one "$cell" "S${grid[$i]}" "$seed" "$rep"; done
  fi
}

BLOCK=0
for SPEC in "42 1" "7 1" "42 2"; do
  read -r SEED REP <<< "$SPEC"
  run_one c1s-400 MAIN "$SEED" "$REP"
  run_one c2-100 MAIN "$SEED" "$REP"
  sweep c1s-400 "$BLOCK" "$SEED" "$REP" "${GRID_C1S[@]}"
  [ "$BLOCK" -lt 2 ] && sweep c2-100 "$BLOCK" "$SEED" "$REP" "${GRID_C2[@]}"
  echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  BLOCK=$(( BLOCK + 1 ))
done
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
grep -ac '^LDROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
echo "LD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
