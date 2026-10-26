#!/bin/bash
# Threading R3 step 0 (docs/status.md §22): the c1d receiver re-profile on
# current main plus the R3 instrument (three more [RXSEC] sections and the
# decoder cost bench). Diagnostic only: ships nothing. Both locks for the
# whole session.
#
#   R3D_ROOT=/home/vibe/r3diag R3D_HARD_DEADLINE=<epoch> \
#     setsid nohup bash r3diag_run.sh > $R3D_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the shipped tree ($R3D_ROOT/src/raptorpath/tools/l1, a
# `git -c core.autocrlf=false -c core.eol=lf archive`). Started as `vibe`;
# `sudo -n` only to run the measured binary and perf.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
crlf_guard lib.sh lib_battery.sh perf_rwm_c.sh topo.sh topo_dual.sh r3diag_run.sh
ROOT="${R3D_ROOT:?R3D_ROOT}"
: "${R3D_HARD_DEADLINE:?R3D_HARD_DEADLINE (epoch seconds)}"
SOFT=$(( R3D_HARD_DEADLINE - 600 ))
SRC="$(cd "$HERE/../../.." && pwd)"
RUN="$ROOT/run"
BIN="$ROOT/bin/raptorpath"
mkdir -p "$RUN/diag" "$ROOT/bin"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/era.txt"; }
# io pressure `full` avg10 / avg60 (/proc/pressure/io): a covariate only
# (§22 Amendment 3), never a gate.
iofull() { awk '$1 == "full" { split($2, a, "="); split($3, b, "="); print a[2], b[2] }' /proc/pressure/io 2>/dev/null; }
procs_running() { awk '$1 == "procs_running" { print $2 }' /proc/stat; }
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg) procs_running=$(procs_running) io_full=$(iofull | tr ' ' /)"; }
sudo -n true 2>/dev/null || { era "ABORT-SUDO"; exit 3; }
# Launch precondition (§22 Amendment 3, the shared rule; supersedes the
# loadavg and io clauses): no cargo / rustc, and `procs_running` ≤ 2 in each
# of 5 samples 2 s apart, checked before the locks are taken (take_lock then
# refuses a held lock).
calm=1
for _ in 1 2 3 4 5; do
  [ "$(procs_running)" -le 2 ] || calm=0
  sleep 2
done
if [ "$(pgrep -xc cargo)" != 0 ] || [ "$(pgrep -xc rustc)" != 0 ] || [ "$calm" = 0 ]; then
  era "ABORT-PRECONDITION $(cotenants)"
  exit 3
fi
LB_TAG="r3diag_run:$$"
LB_LOG="$RUN/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
era "R3D start hard=$R3D_HARD_DEADLINE soft=$SOFT commit=$(cat "$ROOT/COMMIT" 2>/dev/null) $(cotenants)"
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
export CARGO_TARGET_DIR="$ROOT/tgt"

# ── build + the instrument's own tests + the decoder bench ──────────────
t0=$(date +%s)
(cd "$SRC" && "$CARGO" build --release --bin raptorpath) > "$RUN/build.log" 2>&1
rc=$?
era "BUILD rc=$rc wall=$(( $(date +%s) - t0 ))s $(cotenants)"
[ "$rc" = 0 ] || { era "ABORT-BUILD"; release_locks; exit 1; }
cp "$CARGO_TARGET_DIR/release/raptorpath" "$BIN"
sha256sum "$BIN" | tee "$RUN/BINSHA.txt"
t0=$(date +%s)
(cd "$SRC" && "$CARGO" test -p raptorpath --release --lib -- task_obs:: ) > "$RUN/TESTS.txt" 2>&1
rc=$?
era "TESTS task_obs rc=$rc wall=$(( $(date +%s) - t0 ))s $(grep -a 'test result' "$RUN/TESTS.txt" | tail -1)"
[ "$rc" = 0 ] || { era "ABORT-TESTS"; release_locks; exit 1; }
(cd "$SRC" && "$CARGO" test -p raptorpath --release --lib -- --ignored --nocapture \
    bench_in_order_add_symbol_against_the_prune_cadence) > "$RUN/bench.txt" 2>&1
era "BENCH rc=$? $(grep -a -c DECBENCH "$RUN/bench.txt") lines"

# ── measurements run from tmpfs (shared rule; §22 Amendment 2) ──────────
# The binary and the whole run dir (era, rows, perf output) live under
# /dev/shm while anything is measured; the ledgers go to disk after the
# last measurement.
DISK_RUN="$RUN"
SHM="/dev/shm/r3diag-a2139"
rm -rf "$SHM"; mkdir -p "$SHM/diag" "$SHM/bin"
cp "$BIN" "$SHM/bin/raptorpath"
BIN="$SHM/bin/raptorpath"
cp "$RUN/era.txt" "$SHM/era.txt"
RUN="$SHM"
LB_LOG="$RUN/era.txt"
era "TMPFS run=$RUN bin_sha=$(sha256sum "$BIN" | cut -c1-16)"

# ── the diagnostic rows ──────────────────────────────────────────────────
drv() { # cell seed bytes extra-env...
  local cell="$1" seed="$2" bytes="$3" mode
  shift 3
  case "$cell" in c1s) mode=single ;; c1d) mode=dual ;; esac
  sudo -n env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
      -u RWM_RTOBS -u RWM_RDIAG -u RWM_ACKDIAG -u RWM_IO_RT -u RWM_DIAG \
      RWM_GEN=0 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$BIN" SEED="$seed" RWM_DIAG=1 "$@" \
    bash perf_rwm_c.sh c1 c1 bulk "$bytes" 1 "$mode"
}
row() { # cell seed rep
  local cell="$1" seed="$2" rep="$3" t cb rc
  t="$cell-400-s$seed-r$rep"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  cb=$(cotenants)
  # io pressure `full avg10` before / every 2 s during / after the row: a
  # covariate. FLAG-IOSTALL (reported, not void) if any reading is >= 20;
  # VOID-ONDISK if the binary or the run dir is not on /dev/shm.
  local samp="$RUN/diag/$t-io.txt" sp ondisk=""
  case "$BIN:$RUN" in /dev/shm/*:/dev/shm/*) ;; *) ondisk=" VOID-ONDISK" ;; esac
  ( while :; do iofull | cut -d' ' -f1 >> "$samp"; sleep 2; done ) &
  sp=$!
  drv "$cell" "$seed" 400000000 RWM_RTOBS=2 RWM_RDIAG=1 > "$RUN/diag/$t-drv.out" 2>&1
  rc=$?
  kill "$sp" 2>/dev/null; wait "$sp" 2>/dev/null
  local ca io_max void=""
  ca=$(cotenants)
  io_max=$( { cat "$samp" 2>/dev/null; echo "$cb" | sed 's/.*io_full=\([0-9.]*\).*/\1/'; echo "$ca" | sed 's/.*io_full=\([0-9.]*\).*/\1/'; } \
    | awk 'BEGIN { m = 0 } $1 ~ /^[0-9.]+$/ { if ($1 + 0 > m) m = $1 + 0 } END { printf "%.2f", m }')
  awk -v m="$io_max" 'BEGIN { exit !(m >= 20) }' && void=" FLAG-IOSTALL"
  cp /tmp/rwm-c.log "$RUN/diag/$t-c.log" 2>/dev/null; cp /tmp/rwm-s.log "$RUN/diag/$t-s.log" 2>/dev/null
  era "ROW $t rc=$rc $(grep -a -o '"mbps":[0-9.]*' "$RUN/diag/$t-c.log" | tail -1) io_full10_max=$io_max$void$ondisk before[$cb] after[$ca]"
}
rep=0
for seed in 42 7 42; do
  rep=$(( rep + 1 ))
  for cell in c1d c1s; do
    [ "$(date +%s)" -ge "$SOFT" ] && { era "TRUNCATED rows"; break 2; }
    row "$cell" "$seed" "$rep"
  done
done

# ── the c1d server profile (dwarf call graphs) ──────────────────────────
if command -v perf >/dev/null 2>&1 && [ "$(date +%s)" -lt "$SOFT" ]; then
  t="P-c1d"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  drv c1d 42 1000000000 > "$RUN/diag/$t-drv.out" 2>&1 &
  d=$!
  for _ in $(seq 1 300); do grep -q -- '--- RWM-C perf' "$RUN/diag/$t-drv.out" 2>/dev/null && break; sleep 0.1; done
  srv=$(pgrep -o -x raptorpath)
  sleep 3
  era "PERF $t srv=$srv"
  sudo -n perf record -g --call-graph dwarf,16384 -F 999 -p "$srv" -o "$RUN/$t.perf.data" -- sleep 3 > "$RUN/$t.rec.log" 2>&1
  wait "$d"
  cp /tmp/rwm-c.log "$RUN/diag/$t-c.log" 2>/dev/null; cp /tmp/rwm-s.log "$RUN/diag/$t-s.log" 2>/dev/null
  era "RUN $t $(grep -a -o '"mbps":[0-9.]*' "$RUN/diag/$t-c.log" | tail -1) after[$(cotenants)]"
  P="$RUN/$t.perf.data"
  sudo -n perf report -i "$P" --no-children --sort sym --stdio --percent-limit 0.3 > "$RUN/$t.self.txt" 2>/dev/null
  sudo -n perf report -i "$P" --children --sort sym --stdio --percent-limit 0.5 -g none > "$RUN/$t.children.txt" 2>/dev/null
  # Who calls the allocator and memmove (the attribution §16 lacked).
  sudo -n perf report -i "$P" --no-children --sort sym --stdio -g caller,0.5,callee,function,percent \
      -S _int_malloc,__libc_malloc2,_int_free_chunk,malloc_consolidate,__memmove_avx_unaligned_erms,cfree,__libc_free \
      > "$RUN/$t.alloc-callers.txt" 2>/dev/null
  # Folded stacks, for the receiver-rooted breakdown done off the VM.
  sudo -n perf script -i "$P" -F tid,sym 2>/dev/null | gzip -c > "$RUN/$t.script.gz"
  sudo -n rm -f "$P"
else
  era "SKIP PERF"
fi
sudo -n pkill -x raptorpath 2>/dev/null || true
# Ledgers to disk after the last measurement, then drop the tmpfs dir.
sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null
rm -rf "$RUN/bin"
cp -a "$RUN/." "$DISK_RUN/"
rm -rf "$SHM"
RUN="$DISK_RUN"
LB_LOG="$RUN/era.txt"
# The target dir goes while this session still holds both locks (no other
# battery can be running).
rm -rf "$CARGO_TARGET_DIR"
era "END raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
release_locks
touch "$RUN/DONE"
