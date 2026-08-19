#!/bin/bash
# THE SENDER CPU-CEILING BATTERY — the VM driver for goal-gate "MEASUREMENT
# TRUTH item 2 — THE SENDER CPU CEILING (PRE-REGISTRATION, PHASE 1)". That
# block is the CONTRACT: it is scored against, never modified, and no number
# in it may change now that the VM has been touched.
#
#   sudo bash cpuprof_battery.sh <seed> [reps]
#
# ── THE QUESTION, AND WHY IT NEEDS TWO INSTRUMENTS ──────────────────────
# The c9 scored battery established a sender ceiling three independent ways
# and could not take it apart: `CPUCLI` 68.5 ms/MB at c9 and 69.2 at c9h
# (invariant across a 400 Mbit and a 240 Mbit cell), and 1.51 cores / 68.5
# ms/MB = 176.3 Mbit/s against 176.4 measured. Every negative result at that
# cell is confounded by it. THIS BATTERY MEASURES WHERE THE ms/MB GOES.
#
# It runs THREE arms at the SAME cell, and the third is the one that makes
# the other two readable:
#
#   B  BASELINE          no instrument at all. The shipped default binary,
#                        the shipped default env. **THE CEILING'S OWN ARM**:
#                        every ms/MB quoted as the CEILING comes from here and
#                        from nowhere else, because an instrument that costs
#                        CPU at a CPU-BOUND cell moves the number it reports.
#   S  SELF-TIMING       `RWM_CPUPROF=1` on the SAME default binary. Emits one
#                        `[CPUPROF]` line per run: five named seams with their
#                        wall, their entry counts and their share of process
#                        CPU, plus the UNATTRIBUTED remainder.
#   P  PERF              `perf record` attached to the client pid, on a binary
#                        built with `--profile release-prof`. Whole-process
#                        sampling: it sees quinn's endpoint driver, its
#                        `sendmsg`, and rustls/ring's AEAD packet protection —
#                        none of which the `S` arm can reach.
#
# **NEITHER INSTRUMENT SUBSUMES THE OTHER, AND THAT IS THE DESIGN.** `S`
# attributes EXACTLY but only on the sender task; `P` covers everything but
# attributes by sampling into symbols that whole-program LTO has inlined
# together. `S`'s `unattr` column and `P`'s non-sender-task samples are
# MEASURING THE SAME QUANTITY BY DIFFERENT MECHANISMS, and the contract scores
# them against each other rather than promoting one.
#
# ── WHY `perf record -p <pid>` AFTER LAUNCH, AND NOT A WRAPPER ──────────
# The engine runs as `ip netns exec <ns> /usr/bin/time -v -o … env … $BIN perf
# --client …` inside `perf_rwm_c.sh`. Three attachment strategies exist and
# two of them are wrong here:
#
#   1. WRAP `perf_rwm_c.sh` — REJECTED. It would profile BOTH roles: that
#      script launches the `--server` (the perf RECEIVER) in the same process
#      tree. Reading receiver samples into a SENDER ceiling is the §16.14
#      wrong-log trap in profile form. It would also require editing that
#      file, which every other battery in this tree shares.
#   2. WRAP the `ip netns exec` line — REJECTED. Same file edit, plus `perf`
#      would then also sample `ip`, `env` and `/usr/bin/time`, and the
#      `--inherit` default makes the boundary between them and the engine a
#      matter of symbol names rather than of pids.
#   3. **`perf record -p <CLIENT PID>` AFTER LAUNCH — TAKEN.** It profiles the
#      one process whose `CPUCLI` IS the ceiling's numerator, and nothing
#      else. The pid is identified MECHANICALLY — the `raptorpath` process
#      whose `/proc/<pid>/cmdline` contains `--client` — never by "the newest
#      one" or by ordering, because both roles are the same executable and a
#      race there would silently profile the receiver.
#
# `perf` is NOT namespace-scoped: `perf_event_open` takes a pid and works
# across a netns boundary, so the profiler does not need to enter `rp-cli`.
# What it DOES need is root (we have it) and a permissive
# `kernel.perf_event_paranoid`; both are RECORDED in the header, never
# assumed, and a run that could not attach is reported as an INSTRUMENT-FAIL
# rather than as a profile with no samples.
#
# ── THE ATTACH GAP, DISCLOSED RATHER THAN DISCOVERED ────────────────────
# `perf` attaches by POLLING for the client pid, so it starts some hundreds of
# milliseconds after the client does and MISSES THE HEAD OF THE RUN. The
# `P` arm therefore under-samples process startup (cert generation, TLS
# handshake) and the `perf` warm-up object. `PERF_ATTACH_MS` is recorded on
# every invocation so the gap is a COLUMN and not an unknown, and the contract
# reads `P` as a SHAPE (which symbols dominate) rather than as a total. The
# TOTAL comes from `B`'s `CPUCLI`, which has no gap.
#
# ── THE PROFILE-PROFILE HAZARD, AND THE CHECK THAT CLOSES IT ────────────
# `--profile release-prof` exists so `perf` has symbols and line tables.
# It inherits `release` and adds ONLY `debug = 1`; it deliberately does NOT
# set `force-frame-pointers`, which would cost a register and change codegen,
# making the profiled binary a different sender from the one the ceiling was
# measured on. DWARF CFI (`.eh_frame`) is already in release builds
# (`panic = "unwind"` is the default), so `--call-graph dwarf` unwinds without
# frame pointers.
#
# **AND THE CLAIM THAT THE TWO BUILDS ARE THE SAME CODE IS CHECKED, NOT
# ASSUMED.** `text_sha` below hashes the `.text` section of both binaries. If
# they differ, `P`'s profile describes a DIFFERENT sender from `B`'s ceiling
# and the header says so — loudly — instead of a reader discovering it in a
# results table.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
source ./abort_witness.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-6}"
CP_CELLS="${RWM_CP_CELLS:-c9}"
CP_ARMS="${RWM_CP_ARMS:-B S P}"
TAG="${RWM_CP_TAG:-cpuprof}"

ROOT="${RWM_CP_ROOT:-/home/vibe/raptorpath}"
# THE TWO BINARIES. `BIN_REL` is the DEFAULT release build — the one every
# other battery runs and the one the ceiling was measured on. `BIN_PROF` is
# the `release-prof` build and is used by the `P` arm ALONE.
BIN_REL="${RWM_CP_BIN:-$ROOT/target/release/raptorpath}"
BIN_PROF="${RWM_CP_BIN_PROF:-$ROOT/target/release-prof/raptorpath}"

OUTDIR="${RWM_CP_OUTDIR:-/home/vibe/cpuprof}"
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$OUTDIR/diag"
PDIR="$OUTDIR/perf"
mkdir -p "$OUTDIR" "$DDIR" "$PDIR"

# perf sampling. `-F 999` rather than the 4000 default and rather than a round
# 1000: a round frequency lock-steps with the kernel's own periodic timers and
# biases the sample toward whatever runs beside them. 999 Hz on a 1.5-core
# process is ~1500 samples/s, which is ample for a two-name answer and costs
# well under 1 % — and the cost is MEASURED by the B-vs-P contrast rather than
# claimed.
PERF_FREQ="${RWM_CP_FREQ:-999}"
# CALL GRAPHS ARE OPT-IN AND SEPARATE. `--call-graph dwarf` copies a stack
# snapshot per sample (~8-16 kB), which at 1500 samples/s is 12-24 MB/s of
# writes — real overhead at a CPU-BOUND cell, i.e. exactly the arm where an
# instrument's cost distorts the measurand. The DEFAULT is a FLAT leaf
# profile, which is what "name the top two costs" actually needs. Set
# `RWM_CP_GRAPH=1` for the caller-attribution pass, and read it as a separate,
# more expensive arm.
PERF_GRAPH="${RWM_CP_GRAPH:-0}"

# cell -> "scenA scenB mode bytes". TRANSCRIBED from the c9 battery's own
# geometry (`perf_rwm_c.sh` QUAD MODE: `c2 c2 … quad` -> c2 c2 c2 c2), never
# redefined here, so the rows pool with the c9 ledger.
cell_spec() {
  case "$1" in
    c9)  echo "c2 c2 quad   400000000" ;;   # the SYMMETRIC quad — the ceiling's own cell
    c9h) echo "c2 c3 quad   150000000" ;;   # the HETEROGENEOUS quad — the invariance check
    c1)  echo "c1 c1 single 400000000" ;;   # single-path, the item-3 cell, sender-bound too
    *) echo "" ;;
  esac
}
cell_paths() { case "$1" in c9|c9h) echo 4 ;; *) echo 1 ;; esac; }

arm_bin() { case "$1" in P) echo "$BIN_PROF" ;; *) echo "$BIN_REL" ;; esac; }
arm_env() {
  case "$1" in
    S) echo "RWM_CPUPROF=1" ;;
    *) echo "" ;;                 # B and P carry NO instrument env
  esac
}
# The `[CPUPROF]` echo expectation, per arm — the MECHANICAL assertion that
# the gate took. `S` must have it two-sided in `[GATES]` AND must have emitted
# the line; `B` and `P` must have NEITHER. A contaminated control is the
# failure this exists to catch, and at a CPU-bound cell it would be invisible
# in every other column.
arm_wants_cpuprof() { case "$1" in S) echo 1 ;; *) echo 0 ;; esac; }

# ── `.text` EQUALITY: the release-prof build must be the same CODE ───────
# `objcopy` to stdout is not portable across binutils versions; `readelf` +
# `dd` is. Prints a sha256 of the `.text` bytes, or `unreadable`.
text_sha() {
  local f="$1" line off size
  [ -f "$f" ] || { echo "missing"; return 0; }
  line=$(readelf -S -W "$f" 2>/dev/null | grep -E '\.text[[:space:]]' | head -1)
  [ -n "$line" ] || { echo "unreadable"; return 0; }
  off=$(echo "$line" | awk '{print $5}')
  size=$(echo "$line" | awk '{print $6}')
  dd if="$f" bs=1 skip=$((16#$off)) count=$((16#$size)) status=none 2>/dev/null \
    | sha256sum | cut -d' ' -f1
}

# ── THE PERF ATTACH. Returns via the globals PERF_PID / PERF_ATTACH_MS /
#    PERF_TARGET_PID / PERF_FAIL, and NEVER changes control flow: a failed
#    attach produces an instrument-fail line and an arm that still ran.
PERF_PID=""; PERF_ATTACH_MS=""; PERF_TARGET_PID=""; PERF_FAIL=""
perf_attach() { # perf_data_path start_epoch_ms
  local data="$1" t0="$2" i pid cmd
  PERF_PID=""; PERF_ATTACH_MS=""; PERF_TARGET_PID=""; PERF_FAIL=""
  # Poll up to 60 s for the CLIENT. THE PREDICATE IS `--client` IN THE
  # CMDLINE, not recency and not pid order: both roles are the same
  # executable, and profiling the receiver would produce a plausible-looking
  # profile of the wrong process.
  for ((i = 0; i < 300; i++)); do
    for pid in $(pgrep -x raptorpath 2>/dev/null); do
      cmd=$(tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null)
      case "$cmd" in
        *--client*) PERF_TARGET_PID="$pid"; break 2 ;;
      esac
    done
    sleep 0.2
  done
  if [ -z "$PERF_TARGET_PID" ]; then
    PERF_FAIL="no-client-pid"
    return 0
  fi
  local gflag=()
  [ "$PERF_GRAPH" = "1" ] && gflag=(--call-graph dwarf)
  perf record -F "$PERF_FREQ" "${gflag[@]}" -p "$PERF_TARGET_PID" \
      -o "$data" >/dev/null 2>"$data.err" &
  PERF_PID=$!
  PERF_ATTACH_MS=$(( $(date +%s%3N) - t0 ))
  # A perf that died immediately (paranoid, missing PMU, no permission) must
  # be reported as an instrument failure and not as an empty profile.
  sleep 0.5
  kill -0 "$PERF_PID" 2>/dev/null || { PERF_FAIL="perf-died-at-start"; PERF_PID=""; }
  return 0
}

perf_reap() { # perf_data_path name rep
  local data="$1" name="$2" rep="$3"
  [ -n "$PERF_PID" ] || return 0
  # `perf record -p` exits on its own when the target exits. Wait, bounded, then
  # SIGINT (perf's own clean-finish signal — SIGTERM can truncate perf.data).
  local i
  for ((i = 0; i < 100; i++)); do
    kill -0 "$PERF_PID" 2>/dev/null || break
    sleep 0.2
  done
  if kill -0 "$PERF_PID" 2>/dev/null; then
    kill -INT "$PERF_PID" 2>/dev/null || true
    sleep 1
    kill -0 "$PERF_PID" 2>/dev/null && kill -TERM "$PERF_PID" 2>/dev/null
  fi
  wait "$PERF_PID" 2>/dev/null
  PERF_PID=""
  local n
  n=$(perf report -i "$data" --stdio 2>/dev/null | grep -cE '^ *[0-9]+\.[0-9]+%' || true)
  n="${n:-0}"
  echo "PERFCAP $name rep=$rep data=$data target_pid=$PERF_TARGET_PID attach_ms=${PERF_ATTACH_MS:-NA} symbol_rows=$n graph=$PERF_GRAPH" >> "$OUT"
  [ "$n" -eq 0 ] && echo "INSTRUMENT-FAIL-PERF $name rep=$rep (perf.data has no symbolized rows — attach, paranoid or symbolization failed; $(head -2 "$data.err" 2>/dev/null | tr '\n' ' '))" >> "$OUT"
  # The FLAT LEAF PROFILE, committed beside the raw data so the ledger is
  # readable without perf installed on the reader's machine.
  perf report -i "$data" --stdio --no-children --percent-limit 0.20 \
    > "${data%.data}.report.txt" 2>/dev/null || true
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs bin ca cb mode bytes
  envs="$(arm_env "$arm")"; bin="$(arm_bin "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  [ -x "$bin" ] || { echo "MISSING-BINARY $name rep=$REP $bin" >> "$OUT"; return 0; }

  local t0 t0ms; t0=$(date +%s); t0ms=$(date +%s%3N)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG bin=$bin env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt /tmp/rwm-abort.txt

  local pdata="$PDIR/${name}-s${SEED_ARG}-r${REP}.data"
  rm -f "$pdata" "$pdata.err"

  # THE ENGINE, BACKGROUNDED so the profiler can attach to it. Every other
  # battery in this tree runs `perf_rwm_c.sh` in the foreground; this one
  # cannot, because the pid it needs does not exist until several seconds
  # in. The wait below is unbounded on purpose — `perf_rwm_c.sh` has its own
  # 700 s `timeout` on the client and its own EXIT trap.
  #
  # NOTE ON WHAT IS *NOT* SET: `RWM_DIAG`, `RWM_ACKDIAG`, `RWM_WALLDIAG` and
  # `RWM_LATPROBE` are all ABSENT from every arm. The cell is SENDER-CPU-BOUND
  # and those are sender-side instruments; carrying them would put their cost
  # inside the ms/MB this battery exists to decompose. That is a DELIBERATE
  # divergence from the era/latency batteries' env and it is why `B`'s ms/MB is
  # not required to reproduce the c9 ledger's 68.5 exactly — see the contract's
  # ENV-DELTA clause.
  # shellcheck disable=SC2086
  ( env SEED=$SEED_ARG RWM_GEN=0 RWM_BIN="$bin" $envs \
      AW_CELL="$cell" AW_ARM="$arm" AW_REP="$REP" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | tee /tmp/rwm-perf-out.txt > /dev/null ) &
  local ENG_PID=$!

  if [ "$arm" = "P" ]; then
    perf_attach "$pdata" "$t0ms"
    [ -n "$PERF_FAIL" ] && echo "INSTRUMENT-FAIL-PERF-ATTACH $name rep=$REP cause=$PERF_FAIL" >> "$OUT"
  fi

  wait "$ENG_PID"
  [ "$arm" = "P" ] && perf_reap "$pdata" "$name" "$REP"

  grep -E "summary|\"dnf\"|CPU:|GUARD|QCAP|BUSY" /tmp/rwm-perf-out.txt >> "$OUT" 2>/dev/null
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  check_and_parse "$name" "$cell" "$arm"
}

check_and_parse() { # name cell arm
  local name="$1" cell="$2" arm="$3"
  local cpus cpuc secs bytes_done
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  # THE TRANSFER WALL — `seconds` from the run's own summary JSON, NEVER
  # INVOCATION_S (discipline 16). It is the denominator of both goodput and
  # ms/MB, and the c9 calibration measured the invocation wall running
  # 1.08-1.28x the transfer wall.
  secs=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null \
    | grep -o '"seconds":[0-9.]*' | tail -1 | cut -d: -f2)
  bytes_done=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null \
    | grep -o '"bytes":[0-9]*' | tail -1 | cut -d: -f2)

  # ── THE HEADLINE DERIVED QUANTITY, computed HERE so the ledger carries it
  #    even if a parser later changes its mind about a column. This is the
  #    number the whole contract is about.
  #
  #      ms_per_MB = CPUCLI_s * 1000 / (bytes / 1e6)
  #      pred_mbit = cores / ms_per_MB * 8000        [cores = CPUCLI / seconds]
  #
  #    `pred_mbit` is the c9 ceiling arithmetic reproduced verbatim: 1.51
  #    cores / 68.5 ms/MB = 22.0 MB/s = 176.3 Mbit/s. Printing it beside the
  #    MEASURED goodput on every invocation is what makes the ceiling-
  #    validation criterion readable without a join.
  local msmb cores pred meas
  msmb=NA; cores=NA; pred=NA; meas=NA
  if [ -n "$cpuc" ] && [ -n "$secs" ] && [ -n "$bytes_done" ]; then
    read -r msmb cores pred meas <<< "$(awk -v c="$cpuc" -v s="$secs" -v b="$bytes_done" \
      'BEGIN{ if (b>0 && s>0) { mb=b/1e6; m=c*1000/mb; co=c/s;
              printf "%.2f %.3f %.1f %.1f", m, co, (m>0? co/m*8000 : 0), mb*8/s } }')"
  fi
  echo "CEIL $name rep=$REP cpucli=${cpuc:-NA} cpusrv=${cpus:-NA} seconds=${secs:-NA} bytes=${bytes_done:-NA} ms_per_MB=${msmb:-NA} cores=${cores:-NA} pred_mbit=${pred:-NA} meas_mbit=${meas:-NA}" >> "$OUT"

  # ── THE `[CPUPROF]` LINE, verbatim, and its TWO-SIDED gate assertion.
  local want cp_n gates_on gates_off
  want="$(arm_wants_cpuprof "$arm")"
  cp_n=$(grep -c '\[CPUPROF\]' /tmp/rwm-c.log 2>/dev/null || true); cp_n="${cp_n:-0}"
  gates_on=$(grep -c 'RWM_CPUPROF=1' /tmp/rwm-c.log 2>/dev/null || true); gates_on="${gates_on:-0}"
  gates_off=$(grep -c 'RWM_CPUPROF=0' /tmp/rwm-c.log 2>/dev/null || true); gates_off="${gates_off:-0}"
  if [ "$want" -eq 1 ]; then
    [ "$gates_on" -eq 0 ] && echo "ARM-LIVENESS-FAIL $name rep=$REP (no RWM_CPUPROF=1 in [GATES] — the gate did not reach the binary)" >> "$OUT"
    [ "$gates_off" -gt 0 ] && echo "ARM-LIVENESS-FAIL $name rep=$REP (RWM_CPUPROF=0 present on the S arm)" >> "$OUT"
    [ "$cp_n" -eq 0 ] && echo "INSTRUMENT-FAIL-CPUPROF $name rep=$REP (gate armed, no [CPUPROF] line — the teardown emission site was not reached)" >> "$OUT"
    [ "$cp_n" -gt 1 ] && echo "INSTRUMENT-FAIL-CPUPROF $name rep=$REP (${cp_n} [CPUPROF] lines from one sender — the gauge is not one-shot)" >> "$OUT"
  else
    [ "$gates_on" -gt 0 ] && echo "ARM-CONTAMINATION $name rep=$REP (RWM_CPUPROF=1 on a control arm — its ms/MB carries the instrument's own cost)" >> "$OUT"
    [ "$cp_n" -gt 0 ] && echo "ARM-CONTAMINATION $name rep=$REP ([CPUPROF] emitted on a control arm)" >> "$OUT"
  fi
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -h '\[CPUPROF\]' \
    | sed "s/^.*\(\[CPUPROF\]\)/CPUPROFLINE $name rep=$REP \1/" >> "$OUT") || true

  # Liveness shared with every battery in this tree.
  local ac; ac=$(grep -cF "quinn congestion controller: BBR" /tmp/rwm-c.log 2>/dev/null || true); ac="${ac:-0}"
  if [ "$ac" -eq 0 ]; then
    local cause; cause=$(python3 -c "
import sys; sys.path.insert(0, '.')
from abort_witness import cause_or
print(cause_or('/tmp/rwm-abort.txt'))" 2>/dev/null)
    echo "ABORT $name rep=$REP (no CC anchor on the client) abort_cause=${cause:-no_record}" >> "$OUT"
  fi
  echo "LIVENESS $name rep=$REP anchor_cc=$ac cpuprof_lines=$cp_n gates_on=$gates_on gates_off=$gates_off" >> "$OUT"

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
}

# ── PRE-FLIGHT ──────────────────────────────────────────────────────────
[ -x "$BIN_REL" ] || { echo "MISSING BINARY: $BIN_REL" | tee -a "$OUT" >&2; exit 5; }
case " $CP_ARMS " in
  *" P "*)
    [ -x "$BIN_PROF" ] || { echo "MISSING BINARY: $BIN_PROF — build it with 'cargo build --profile release-prof'" | tee -a "$OUT" >&2; exit 5; }
    command -v perf >/dev/null 2>&1 || { echo "MISSING TOOL: perf (apt-get install linux-perf / linux-tools-\$(uname -r))" | tee -a "$OUT" >&2; exit 5; }
    ;;
esac
if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2; exit 3
fi

{
  echo "=== CPU-CEILING BATTERY seed=$SEED_ARG reps=$REPS cells='$CP_CELLS' arms='$CP_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT ${RWM_CP_CONTRACT:-goal-gate \"MEASUREMENT TRUTH item 2 — THE SENDER CPU CEILING (PRE-REGISTRATION, PHASE 1)\"}"
  echo "=== ARMS  B = no instrument (THE CEILING'S OWN ARM) | S = RWM_CPUPROF=1 (self-timing) | P = perf record -p <client> on the release-prof build"
  echo "=== THE CEILING'S ms/MB IS READ OFF ARM B AND OFF NOTHING ELSE. An instrument that costs CPU at a CPU-BOUND cell moves the number it reports; B is what bounds that, and S-vs-B and P-vs-B ARE the instrument-cost measurement."
  echo "=== NO DIAG/ACKDIAG/WALLDIAG/LATPROBE ON ANY ARM — all of them are sender-side costs inside the quantity under decomposition. This env DIFFERS from the era and latency batteries deliberately."
  echo "=== perf attaches by PID, selected on '--client' in /proc/<pid>/cmdline. Both roles are the same executable; recency would race."
  echo "=== perf's ATTACH GAP is recorded per invocation (attach_ms). P is read as a SHAPE, never as a total; the total is B's CPUCLI."
  echo "=== release binary $BIN_REL sha256 $(sha256sum "$BIN_REL" 2>/dev/null | cut -d' ' -f1)"
  echo "=== release-prof binary $BIN_PROF sha256 $(sha256sum "$BIN_PROF" 2>/dev/null | cut -d' ' -f1)"
  echo "=== .text release      $(text_sha "$BIN_REL")"
  echo "=== .text release-prof $(text_sha "$BIN_PROF")"
  if [ "$(text_sha "$BIN_REL")" = "$(text_sha "$BIN_PROF")" ]; then
    echo "=== TEXT-EQUAL YES — the profiled binary is the SAME CODE as the ceiling's binary; P's shape may be attributed to B's total."
  else
    echo "=== TEXT-EQUAL NO — *** the release-prof build is NOT the same code as the release build. P describes a DIFFERENT sender from B's ceiling and every attribution below is qualified by this line. ***"
  fi
  echo "=== source $(cat "$ROOT/COMMIT" 2>/dev/null)"
  echo "=== perf $(perf --version 2>/dev/null || echo ABSENT) freq=$PERF_FREQ graph=$PERF_GRAPH"
  echo "=== perf_event_paranoid $(cat /proc/sys/kernel/perf_event_paranoid 2>/dev/null || echo unknown) kptr_restrict $(cat /proc/sys/kernel/kptr_restrict 2>/dev/null || echo unknown)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  echo "=== co-tenant $(pgrep -c -x kwin_x11 2>/dev/null || echo 0) kwin_x11 / $(pgrep -c -x sddm 2>/dev/null || echo 0) sddm"
  echo "=== steal $(awk '/^cpu /{print \"user=\"$2\" sys=\"$4\" idle=\"$5\" steal=\"$9}' /proc/stat 2>/dev/null)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

# Arms INTERLEAVED within one rep of one cell, on the same freshly built
# topology — so B, S and P sit adjacent and the instrument-cost contrast is
# PAIRED WITHIN REP rather than across a session's drift.
for REP in $(seq 1 "$REPS"); do
  for CELL in $CP_CELLS; do
    for ARM in $CP_ARMS; do
      case " $CP_ARMS " in *" $ARM "*) run_topo "$CELL" "$ARM" ;; esac
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $CP_CELLS; do
  for A in $CP_ARMS; do
    N=$(grep -c "^CEIL $CELL-$A " "$OUT" || true)
    echo "ARMCOUNT $CELL-$A rows=$N/$REPS" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "CPUPROF-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo CPUPROF-BATTERY-DONE-$SEED_ARG
