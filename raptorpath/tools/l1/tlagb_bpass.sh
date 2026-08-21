#!/bin/bash
# THE T-LAG BATTERY — THE CLAUSE-B PASS. THE ONLY SCRIPT THAT SETS THE DUMP.
#
#   sudo nohup bash tlagb_bpass.sh >/home/vibe/tlagb/bpass.out 2>&1 &
#
# ONE REP PER CELL, seed 42, 5 invocations, `RWM_GEN=0` — the same seat and the
# same cells as the scored pass, and DELIBERATELY a different session from it.
#
# WHY IT IS A SEPARATE PASS AND NOT A COLUMN OF THE BATTERY. Clause B is scored
# against the RAW RTT SAMPLE STREAM, and the only way to see that stream is
# `RWM_RTT_DUMP=1`, which emits one `[RTTDUMP] p=<id> t0=<us> n=<k>
# d=<dt,rtt;...>` line per path per interval on SENDER stderr. That is
# MEGABYTES of writes into the endpoint whose own dispersion the S and C
# clauses measure. A battery that ran the dump would be measuring its own
# instrument, so `tlagb_battery.sh` UNSETS the gate and ASSERTS it reads 0
# (`TLAGB-DUMP-ON-FAIL`), and this script — alone — turns it on.
#
# **NOTHING HERE IS AN S-CLAUSE OR C-CLAUSE DATUM.** The gauge columns emitted
# during this pass are produced under a perturbed sender and do not pool with
# the scored ledger. What this pass produces is the RTT stream itself.
#
# WHAT IT WRITES, PER INVOCATION:
#   * the FULL client log, preserved (the [RTTDUMP] lines live in it), at
#     /home/vibe/tlagb/dump/<cell>-s42-r1-c.log — these are MEGABYTES, so they
#     are gzipped at the end of the run and read as .log.gz thereafter.
#   * TLAGB-BPASS-GATES <cell> dump=<0|1> max=<n> — the resolved [GATES] echo
#     of `RWM_RTT_DUMP` and `RWM_RTT_DUMP_MAX`. `max` is the truncation budget
#     and the reader needs the RESOLVED value, not the default it assumes.
#   * TLAGB-BPASS-CAP <cell> p=<id> capped=<0|1> — per path, whether a
#     `[RTTDUMP-CAP] p=<id> ...` line appeared. A capped path's stream is
#     TRUNCATED, and a clause-B functional read off a truncated stream is a
#     censored reading that must be declared as one, not silently averaged.
#
# THE CONTAMINATION WITNESSES ARE THE BATTERY'S, UNCHANGED, BECAUSE A
# CONTAMINATED B PASS MUST BE DETECTABLE TOO:
#   W1  [RFA] gen= on the receiver               must read 0
#   W2  [PFRAC] lines on the sender              must be 0
#   W-DUMP  [GATES] RWM_RTT_DUMP=1 here — the INVERSE of the battery's
#           assertion, and just as hard: a B pass with the dump off produced no
#           stream at all and its ledger rows are void.
#
# WATCHER NOTE: `pgrep -f tlagb_bpass.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table (MEASUREMENT DISCIPLINE 13).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-invocation abort tolerance (discipline 7)

SEED_ARG=42
TAG="${RWM_TLAGB_TAG:-tlagb-bpass}"
BP_CELLS="${RWM_TLAGB_CELLS:-c1 c7 c8 c8L sc2}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUTDIR=/home/vibe/tlagb
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DUMPDIR="$OUTDIR/dump"
mkdir -p "$OUTDIR" "$DUMPDIR"
rm -f "$OUTDIR/DONE-BPASS" "$OUTDIR/FAILED-BPASS"

# INHERITANCE DEFEATS AN ALLOWLIST (gate-forwarding audit, PROBE 0). The two
# clock gates are unset here exactly as in the scored battery — this pass is
# the shipped stack plus the dump, and nothing else.
unset RWM_ALPHA_OVERRIDE
unset RWM_QUANTILE_CLOCKS

# Cells TRANSCRIBED verbatim from tlagb_battery.sh, which took them verbatim
# from `ccand_battery.sh:202-215`. A cell that differs from the ledger's cell
# is a different cell and its stream does not pair with the ledger's rows.
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual   200000000" ;;
    c8)  echo "c2 c3 dual    25000000" ;;
    c8L) echo "c2 c3 dual   200000000" ;;
    *) echo "" ;;
  esac
}

REP=1

check_bpass() { # cell
  local cell="$1" name="$1"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log

  # Scoped to the [GATES] line: the per-mechanism ACTIVE echoes' own prose
  # contains literal `RWM_*=0` strings (the flip battery's amendment-1 lesson).
  local gl_c gl_s
  gl_c=$(grep "\[GATES\]" "$C" 2>/dev/null | tail -1)
  gl_s=$(grep "\[GATES\]" "$S" 2>/dev/null | tail -1)

  # ABORT-CAUSE FIRST. No [GATES] on EITHER endpoint = ABORT: no stream, no
  # liveness verdict, and NOT in any denominator.
  if [ -z "$gl_c" ] && [ -z "$gl_s" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi

  # ── W-DUMP, INVERTED: the dump must be ON, and its budget RECORDED ───────
  local rd_c dmax
  rd_c=$(printf '%s' "$gl_c" | grep -o "RWM_RTT_DUMP=[01]" | sed 's/.*=//')
  rd_c="${rd_c:-0}"
  dmax=$(printf '%s' "$gl_c" | grep -o "RWM_RTT_DUMP_MAX=[0-9]*" | sed 's/.*=//')
  dmax="${dmax:--}"
  echo "TLAGB-BPASS-GATES $name dump=$rd_c max=$dmax" >> "$OUT"
  [ "$rd_c" != "1" ] \
    && echo "TLAGB-BPASS-DUMP-OFF-FAIL $name rep=$REP cli-dump='$rd_c' (no RTT stream was emitted; row VOID)" >> "$OUT"

  # ── W1: the receiver's [RFA] gen= must read 0 (the plain-window seat) ────
  # The ONLY direct engine echo of `window_generation` that exists, which is
  # why RWM_FDIAG is load-bearing in the instrument set.
  local rfa_bad rfa_n
  rfa_n=$(grep -c "\[RFA\] gen=" "$S" 2>/dev/null); rfa_n="${rfa_n:-0}"
  rfa_bad=$(grep -o "\[RFA\] gen=[0-9]*" "$S" 2>/dev/null | grep -cv "gen=0"); rfa_bad="${rfa_bad:-0}"
  echo "W1 $name rep=$REP rfa_lines=$rfa_n nonzero_gen=$rfa_bad" >> "$OUT"
  [ "$rfa_bad" -gt 0 ] \
    && echo "W1-FAIL $name rep=$REP (${rfa_bad} [RFA] lines with gen!=0 — generation leaked in; row VOID)" >> "$OUT"

  # ── W2: [PFRAC] on the sender must be absent ────────────────────────────
  local pf_n
  pf_n=$(grep -c "\[PFRAC\]" "$C" 2>/dev/null); pf_n="${pf_n:-0}"
  echo "W2 $name rep=$REP pfrac_lines=$pf_n" >> "$OUT"
  [ "$pf_n" -gt 0 ] \
    && echo "W2-FAIL $name rep=$REP (${pf_n} [PFRAC] lines on the sender; row VOID)" >> "$OUT"

  # The instruments must be armed on BOTH endpoints or the stream is void.
  local i got_c got_s
  for i in RWM_DIAG RWM_FDIAG RWM_ACKDIAG RWM_WALLDIAG; do
    got_c=$(printf '%s' "$gl_c" | grep -o "$i=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$i=[01]")
    { [ "$got_c" != "$i=1" ] || [ "$got_s" != "$i=1" ]; } \
      && echo "INSTRUMENT-FAIL-GATE $name rep=$REP gate=$i cli='$got_c' srv='$got_s'" >> "$OUT"
  done

  # ── THE STREAM ITSELF, AND ITS TRUNCATION WITNESS ───────────────────────
  # Per path: how many [RTTDUMP] lines it produced, and whether a
  # [RTTDUMP-CAP] line named it. The cap is per path, so the answer is per
  # path — a run where one path capped and three did not is three usable
  # clause-B legs and one censored one, not a failed run.
  local pids p nd capped
  pids=$( { grep -o "\[RTTDUMP\] p=[0-9]*" "$C" 2>/dev/null
            grep -o "\[RTTDUMP-CAP\] p=[0-9]*" "$C" 2>/dev/null; } \
          | sed 's/.*p=//' | sort -u )
  if [ -z "$pids" ]; then
    echo "TLAGB-BPASS-NOSTREAM $name rep=$REP (no [RTTDUMP] lines on the sender at all)" >> "$OUT"
  fi
  for p in $pids; do
    nd=$(grep -c "\[RTTDUMP\] p=$p " "$C" 2>/dev/null); nd="${nd:-0}"
    capped=0
    grep -q "\[RTTDUMP-CAP\] p=$p " "$C" 2>/dev/null && capped=1
    echo "TLAGB-BPASS-CAP $name p=$p capped=$capped" >> "$OUT"
    echo "TLAGB-BPASS-LINES $name p=$p dumplines=$nd" >> "$OUT"
  done

  # The band is a CONFIGURATION witness here too, but this pass writes no
  # goodput clause and the dump itself perturbs the sender — so the plateau is
  # RECORDED and not treated as an abort of a clause-B stream.
  local mb
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  echo "TLAGB-BPASS-MBPS $name rep=$REP mbps=$mb (dump ON perturbs the sender — NOT a goodput datum)" >> "$OUT"
}

run_cell() { # cell
  local cell="$1" name="$1"
  local ca cb mode bytes
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP cell=$name seed=$SEED_ARG spec=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # cell's log and pass its liveness gate — or its stream — on it.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt \
        /tmp/rwm-ping.txt /tmp/rwm-ping-0.txt /tmp/rwm-ping-1.txt \
        /tmp/rwm-ping-2.txt /tmp/rwm-ping-3.txt

  env SEED=$SEED_ARG RWM_GEN=0 RWM_RTT_DUMP=1 \
      RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT" || true
  RC=${PIPESTATUS[0]}
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$RC" >> "$OUT"

  check_bpass "$cell"

  # THE WHOLE CLIENT LOG, PRESERVED. The [RTTDUMP] lines ARE the measurement,
  # and the driver's `trap cleanup EXIT` destroys the namespaces the instant it
  # returns — so this is copied now, in full, under a rep-unique name. These
  # are MEGABYTES; they are gzipped after the loop.
  cp /tmp/rwm-c.log "$DUMPDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null \
    || echo "TLAGB-BPASS-CAPTURE-MISSING $name rep=$REP (no client log to preserve)" >> "$OUT"
  cp /tmp/rwm-s.log "$DUMPDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-abort.txt "$DUMPDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
}

{
  echo "=== TLAGB B-PASS seed=$SEED_ARG reps=1 $(date -u +%FT%TZ)"
  echo "PURPOSE clause B only — the RAW RTT SAMPLE STREAM via RWM_RTT_DUMP=1"
  echo "NOT-A-DATUM no S-clause or C-clause row is scored from this pass (the dump perturbs the sender)"
  echo "CELLS $BP_CELLS"
  echo "DUMP  RWM_RTT_DUMP=1 — THIS IS THE ONLY SCRIPT THAT SETS IT"
  echo "SEAT  plain window (RWM_GEN=0)"
  echo "ENV   RWM_GEN=0 RWM_RTT_DUMP=1 RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1"
  echo "CAPTURES full client logs under $DUMPDIR (gzipped after the run)"
  echo "BIN $BIN"
  echo "SHA256 $(sha256sum "$BIN" 2>/dev/null)"
  echo "COMMIT $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "KERNEL $(uname -r)"
  echo "UPTIME $(uptime)"
  echo "COTENANT kwin=$(pgrep -c kwin_x11 2>/dev/null || echo 0) sddm=$(pgrep -c sddm 2>/dev/null || echo 0)"
  echo "CPU $(lscpu | grep -E 'Model name' | head -1)"
} >> "$OUT"

RC=0
echo "TLAGB-BPASS start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/bpass-era.txt"
for CELL in $BP_CELLS; do
  run_cell "$CELL"
done
echo "TLAGB-BPASS end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/bpass-era.txt"

# The preserved client logs are megabytes each — compress them in place, once,
# after the last invocation, so no run competes with gzip for the box.
for CELL in $BP_CELLS; do
  F="$DUMPDIR/${CELL}-s${SEED_ARG}-r1-c.log"
  if [ -f "$F" ]; then
    SZ=$(stat -c %s "$F" 2>/dev/null); SZ="${SZ:-0}"
    gzip -f "$F" \
      && echo "TLAGB-BPASS-GZ $CELL raw_bytes=$SZ -> ${F}.gz" >> "$OUT" \
      || echo "TLAGB-BPASS-GZ-FAIL $CELL (left uncompressed at $F)" >> "$OUT"
  else
    echo "TLAGB-BPASS-GZ-MISSING $CELL (no capture at $F)" >> "$OUT"
  fi
done

echo "=== CELLCOUNTS (invocations, NOT clause-B legs) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $BP_CELLS; do
  N=$(grep -c "^TLAGB-BPASS-GATES $CELL " "$OUT" || true); N="${N:-0}"
  echo "CELLCOUNT $CELL rows=$N/1" >> "$OUT"
  [ "$N" -eq 0 ] && echo "CELL-VANISHED $CELL" >> "$OUT"
done
echo "TLAGB-BPASS-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"

# ── THE SENTINEL IS EARNED, NOT UNCONDITIONAL ────────────────────────────
#
# The calibration script's lesson, applied here unchanged: MEASURED 2026-08-21,
# a CRLF trap in `lib.sh` made the battery exit before its first invocation and
# an unconditional `touch` at the end of the driver reported that as a
# completed run. Discipline 13 tells a watcher to watch the SENTINEL, so the
# sentinel has to mean something. DONE-BPASS is written only if this pass's own
# DONE line is in its own ledger; otherwise FAILED-BPASS, which is a sentinel
# too — one that says the opposite thing.
if [ -s "$OUT" ] && grep -q "TLAGB-BPASS-DONE" "$OUT"; then
  touch "$OUTDIR/DONE-BPASS"
  echo TLAGB-BPASS-DONE
else
  {
    echo "TLAGB-BPASS FAILED $(date -u +%FT%TZ)"
    echo "  no ledger at $OUT, or no TLAGB-BPASS-DONE line in it."
    echo "  THE B PASS DID NOT RUN. Check for a CRLF trap in tools/l1 first:"
    echo "    python3 -c 'print(open(\"lib.sh\",\"rb\").read().count(b\"\\r\\n\"))'"
  } | tee -a "$OUTDIR/bpass-era.txt"
  touch "$OUTDIR/FAILED-BPASS"
  echo TLAGB-BPASS-FAILED
  exit 5
fi
