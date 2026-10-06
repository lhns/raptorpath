#!/bin/bash
# Threading Q3 step 1 — the diagnostic session (docs/status.md §16). NOT a
# scored battery: one instrumented binary (`RWM_RTOBS=2`, the deep gauge),
# a short matrix of single invocations, per-CPU /proc/stat sampling around
# each, and two system-wide `perf record`s.
#
#   Q3_ROOT=/home/vibe/q3diag setsid nohup bash q3diag_run.sh \
#       > $Q3_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced tree ($Q3_ROOT/src/raptorpath/tools/l1). Started as
# `vibe`; `sudo -n` only for the measurement stages. Both operator locks are
# held for the whole session (build included). Sentinel: $RUN/DONE or
# $RUN/FAILED (cause in era.txt).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
crlf_guard lib.sh lib_battery.sh perf_rwm_c.sh topo.sh topo_dual.sh q3diag_run.sh
ROOT="${Q3_ROOT:?Q3_ROOT}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
BIN="$ROOT/bin/raptorpath"
HARD=$(( $(date +%s) + ${Q3_BUDGET_S:-5400} ))
mkdir -p "$RUN" "$ROOT/bin"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/era.txt"; }
sudo -n true 2>/dev/null || { era "ABORT-SUDO"; exit 3; }

LB_TAG="q3diag_run:$$"
LB_LOG="$RUN/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
trap 'release_locks; touch "$RUN/FAILED"' INT TERM
fail() { era "FAILED: $*"; touch "$RUN/FAILED"; release_locks; exit 5; }

# ── build (fresh target; line tables for perf's symbolization only) ──────
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "build start commit=$(cat "$SRC/COMMIT" 2>/dev/null)"
T0=$(date +%s)
( cd "$SRC" && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only "$CARGO" build --release --bin raptorpath ) \
    > "$RUN/build.log" 2>&1 || fail "ABORT-BUILD"
cp "$SRC/target/release/raptorpath" "$BIN" || fail "copy"
era "build done wall=$(( $(date +%s) - T0 ))s sha256=$(sha256sum "$BIN" | cut -d' ' -f1)"
( cd "$SRC" && "$CARGO" test -p raptorpath --release --lib -- task_obs runtime_obs gates:: io_owner ) \
    > "$RUN/test.log" 2>&1
era "tests rc=$? $(grep -a '^test result:' "$RUN/test.log" | tail -1)"

# ── per-CPU /proc/stat sampler (10 Hz) around one invocation ─────────────
statmon() { # out
  while :; do
    echo "T $(date +%s.%N)"
    grep '^cpu' /proc/stat
    sleep 0.1
  done > "$1"
}

run_one() { # tag cell rtobs diag [perf]
  local tag="$1" cell="$2" rtobs="$3" diag="$4" perf="${5:-0}" ca cb mode bytes
  case "$cell" in
    c1s) ca=c1; cb=c1; mode=single; bytes=400000000 ;;
    c1d) ca=c1; cb=c1; mode=dual; bytes=400000000 ;;
    c2)  ca=c2; cb=c2; mode=single; bytes=100000000 ;;
    c8)  ca=c2; cb=c3; mode=dual; bytes=100000000 ;;
  esac
  [ "$(date +%s)" -lt "$HARD" ] || { era "SKIP-DEADLINE $tag"; return; }
  local base="$RUN/$tag"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  statmon "$base-stat.txt" &
  local mon=$!
  local envs=(RWM_RTOBS="$rtobs" RWM_GEN=0 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$BIN" SEED=42)
  [ "$diag" = "1" ] && envs+=(RWM_DIAG=1)
  local t0; t0=$(date +%s)
  if [ "$perf" = "1" ]; then
    sudo -n env -u RWM_DIAG -u RWM_RTOBS -u RWM_EMIT_BATCH -u RWM_EMIT_BURST -u RWM_IO_RT "${envs[@]}" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > "$base-drv.out" 2>&1 &
    local drv=$!
    # Skip bring-up and the first ~2 s of the transfer, then sample 2 s.
    for _ in $(seq 1 300); do grep -q -- '--- RWM-C perf' "$base-drv.out" 2>/dev/null && break; sleep 0.1; done
    sleep 3
    sudo -n perf record -a -g --call-graph dwarf,16384 -F 499 -o "$base.perf.data" -- sleep 2 \
      > "$base-perfrec.log" 2>&1
    wait "$drv"
  else
    sudo -n env -u RWM_DIAG -u RWM_RTOBS -u RWM_EMIT_BATCH -u RWM_EMIT_BURST -u RWM_IO_RT "${envs[@]}" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > "$base-drv.out" 2>&1
  fi
  local rc=$?
  kill "$mon" 2>/dev/null
  cp /tmp/rwm-c.log "$base-c.log" 2>/dev/null
  cp /tmp/rwm-s.log "$base-s.log" 2>/dev/null
  era "RUN $tag cell=$cell rtobs=$rtobs diag=$diag perf=$perf rc=$rc wall=$(( $(date +%s) - t0 ))s $(grep -a -o '"mbps":[0-9.]*' "$base-c.log" | tail -1)"
}

era "matrix start load=$(cut -d' ' -f1-3 /proc/loadavg)"
for r in 1 2 3; do
  run_one "A-c1s-r$r" c1s 2 1
  run_one "A-c1d-r$r" c1d 2 1
done
for r in 1 2; do
  run_one "B-c1s-r$r" c1s 2 0
  run_one "B-c1d-r$r" c1d 2 0
  run_one "C-c1s-r$r" c1s 1 1
  run_one "C-c1d-r$r" c1d 1 1
done
run_one "D-c2-r1" c2 2 1
run_one "D-c8-r1" c8 2 1
run_one "E-c1s-perf" c1s 1 1 1
run_one "E-c1d-perf" c1d 1 1 1

# perf reports (inside the locks: they are CPU-heavy)
for t in E-c1s-perf E-c1d-perf; do
  [ -f "$RUN/$t.perf.data" ] || continue
  sudo -n perf report -i "$RUN/$t.perf.data" --no-children --sort comm,dso,sym --stdio --percent-limit 0.4 \
      > "$RUN/$t.self.txt" 2>/dev/null
  sudo -n perf report -i "$RUN/$t.perf.data" --children --sort sym --stdio --percent-limit 1 -g none \
      > "$RUN/$t.children.txt" 2>/dev/null
  sudo -n perf report -i "$RUN/$t.perf.data" --no-children --sort comm --stdio \
      > "$RUN/$t.comm.txt" 2>/dev/null
  sudo -n rm -f "$RUN/$t.perf.data"
done
sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null
era "END raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
release_locks
touch "$RUN/DONE"
