#!/bin/bash
# Threading R3 step 0, profile re-read (docs/status.md §22 Amendment 4):
# H-alloc (a) only. One 1 GB c1d transfer; the server is `perf record`ed
# (dwarf) for 3 s in the steady state, attached to the pid perf_rwm_c.sh
# itself started (its abort-witness `srv_pid`), not a pgrep guess. Fail-fast
# witness: the recording must hold samples on the server's `rp-w-*` worker
# threads, else ABORT-PERF-ATTACH. Diagnostic only: ships nothing.
#
#   R3D_ROOT=<root> R3D_HARD_DEADLINE=<epoch> bash r3prof_run.sh
# (normally started by r3diag_wait.sh with R3D_SCRIPT=r3prof_run.sh)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
crlf_guard lib.sh lib_battery.sh perf_rwm_c.sh topo.sh topo_dual.sh abort_witness.sh r3prof_run.sh
ROOT="${R3D_ROOT:?R3D_ROOT}"
: "${R3D_HARD_DEADLINE:?R3D_HARD_DEADLINE (epoch seconds)}"
SRC="$(cd "$HERE/../../.." && pwd)"
RUN="$ROOT/run"
BIN="$ROOT/bin/raptorpath"
mkdir -p "$RUN" "$ROOT/bin"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/era.txt"; }
iofull() { awk '$1 == "full" { split($2, a, "="); split($3, b, "="); print a[2], b[2] }' /proc/pressure/io 2>/dev/null; }
procs_running() { awk '$1 == "procs_running" { print $2 }' /proc/stat; }
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg) procs_running=$(procs_running) io_full=$(iofull | tr ' ' /)"; }
sudo -n true 2>/dev/null || { era "ABORT-SUDO"; exit 3; }
# The shared launch precondition (§22 Amendment 3).
calm=1
for _ in 1 2 3 4 5; do
  [ "$(procs_running)" -le 2 ] || calm=0
  sleep 2
done
if [ "$(pgrep -xc cargo)" != 0 ] || [ "$(pgrep -xc rustc)" != 0 ] || [ "$calm" = 0 ]; then
  era "ABORT-PRECONDITION $(cotenants)"
  exit 3
fi
LB_TAG="r3prof_run:$$"
LB_LOG="$RUN/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
era "R3P start hard=$R3D_HARD_DEADLINE commit=$(cat "$ROOT/COMMIT" 2>/dev/null) $(cotenants)"
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
export CARGO_TARGET_DIR="$ROOT/tgt"

t0=$(date +%s)
(cd "$SRC" && "$CARGO" build --release --bin raptorpath) > "$RUN/build.log" 2>&1
rc=$?
era "BUILD rc=$rc wall=$(( $(date +%s) - t0 ))s $(cotenants)"
[ "$rc" = 0 ] || { era "ABORT-BUILD"; release_locks; exit 1; }
cp "$CARGO_TARGET_DIR/release/raptorpath" "$BIN"
sha256sum "$BIN" | tee "$RUN/BINSHA.txt"

# ── the measurement, from tmpfs (§22 Amendment 2) ───────────────────────
DISK_RUN="$RUN"
SHM="/dev/shm/r3prof-a2139"
rm -rf "$SHM"; mkdir -p "$SHM/diag" "$SHM/bin"
cp "$BIN" "$SHM/bin/raptorpath"; BIN="$SHM/bin/raptorpath"
cp "$RUN/era.txt" "$SHM/era.txt"; RUN="$SHM"; LB_LOG="$RUN/era.txt"
era "TMPFS run=$RUN bin_sha=$(sha256sum "$BIN" | cut -c1-16)"

t="P-c1d"
rm -f /tmp/rwm-c.log /tmp/rwm-s.log
cb=$(cotenants)
sudo -n env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
    -u RWM_RTOBS -u RWM_RDIAG -u RWM_ACKDIAG -u RWM_IO_RT -u RWM_DIAG \
    RWM_GEN=0 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$BIN" SEED=42 RWM_DIAG=1 \
    AW_FILE="$RUN/aw.txt" \
  bash perf_rwm_c.sh c1 c1 bulk 1000000000 1 dual > "$RUN/diag/$t-drv.out" 2>&1 &
d=$!
srv=""
for _ in $(seq 1 300); do
  srv=$(sudo -n sed -n 's/^srv_pid=//p' "$RUN/aw.txt" 2>/dev/null | tail -1)
  [ -n "$srv" ] && grep -q -- '--- RWM-C perf' "$RUN/diag/$t-drv.out" 2>/dev/null && break
  sleep 0.1
done
sleep 3
comm=$(cat "/proc/$srv/comm" 2>/dev/null)
era "PERF $t srv_pid=${srv:-none} comm=${comm:-gone} before[$cb]"
sudo -n perf record -g --call-graph dwarf,16384 -F 999 -p "${srv:-0}" -o "$RUN/$t.perf.data" -- sleep 3 > "$RUN/$t.rec.log" 2>&1
prc=$?
wait "$d"
ca=$(cotenants)
cp /tmp/rwm-c.log "$RUN/diag/$t-c.log" 2>/dev/null; cp /tmp/rwm-s.log "$RUN/diag/$t-s.log" 2>/dev/null
era "RUN $t perf_rc=$prc $(grep -a -o '"mbps":[0-9.]*' "$RUN/diag/$t-c.log" | tail -1) after[$ca]"
P="$RUN/$t.perf.data"
# Fail-fast attach witness: samples on the server's rp-w-* worker threads.
sudo -n perf report -i "$P" --sort comm --stdio > "$RUN/$t.comm.txt" 2>/dev/null
nw=$(grep -c ' rp-w-' "$RUN/$t.comm.txt" 2>/dev/null)
if [ "$prc" != 0 ] || [ "${nw:-0}" = 0 ]; then
  era "ABORT-PERF-ATTACH perf_rc=$prc rp_w_lines=${nw:-0}"
else
  era "PERF-ATTACH-OK rp_w_lines=$nw"
  sudo -n perf report -i "$P" --no-children --sort sym --stdio --percent-limit 0.3 > "$RUN/$t.self.txt" 2>/dev/null
  sudo -n perf report -i "$P" --children --sort sym --stdio --percent-limit 0.5 -g none > "$RUN/$t.children.txt" 2>/dev/null
  sudo -n perf report -i "$P" --no-children --sort sym --stdio -g caller,0.5,callee,function,percent \
      -S _int_malloc,__libc_malloc2,_int_free_chunk,malloc_consolidate,__memmove_avx_unaligned_erms,cfree,__libc_free \
      > "$RUN/$t.alloc-callers.txt" 2>/dev/null
  sudo -n perf script -i "$P" -F tid,sym 2>/dev/null | gzip -c > "$RUN/$t.script.gz"
fi
sudo -n rm -f "$P"
sudo -n pkill -x raptorpath 2>/dev/null || true
sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null
rm -rf "$RUN/bin"
cp -a "$RUN/." "$DISK_RUN/"
rm -rf "$SHM"
RUN="$DISK_RUN"; LB_LOG="$RUN/era.txt"
rm -rf "$CARGO_TARGET_DIR"
era "END raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
release_locks
touch "$RUN/DONE"
