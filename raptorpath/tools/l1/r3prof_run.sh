#!/bin/bash
# Threading R3 step 0, profile re-read (docs/status.md §22 Amendments 4-5):
# H-alloc (a) only. One 1 GB c1d transfer; the server is `perf record`ed
# (dwarf) for 3 s in the steady state, attached to the pid perf_rwm_c.sh
# itself started (its abort-witness `srv_pid`). Amendment 5: the binary is
# `--profile release-prof` (release + line tables, so dwarf unwinding works
# in user code) with its `.text` compared to the `--release` build's
# (ABORT-TEXT on a mismatch); `perf script -F tid,ip,sym` (call chains); a
# dry run first (a short c1d transfer, 5 s record, fold on the VM: no
# `run_receiver` samples is ABORT-FOLD-DRY); the real fold must find
# samples under `run_receiver` and `add_symbol` (else ABORT-FOLD).
# Fail-fast attach witness (Amendment 4): samples on `rp-w-*` threads, else
# ABORT-PERF-ATTACH. Diagnostic only: ships nothing.
#
#   R3D_ROOT=<root> R3D_HARD_DEADLINE=<epoch> bash r3prof_run.sh
# (normally started by r3diag_wait.sh with R3D_SCRIPT=r3prof_run.sh)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
crlf_guard lib.sh lib_battery.sh perf_rwm_c.sh topo.sh topo_dual.sh abort_witness.sh r3prof_run.sh r3diag_fold.py
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
finish() { # leave the VM clean; ledgers to disk; locks released
  sudo -n pkill -x raptorpath 2>/dev/null || true
  if [ -n "${DISK_RUN:-}" ]; then
    sudo -n rm -f "$RUN"/*.perf.data
    sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null
    rm -rf "$RUN/bin"
    cp -a "$RUN/." "$DISK_RUN/"
    rm -rf "$SHM"
    RUN="$DISK_RUN"; LB_LOG="$RUN/era.txt"
  fi
  rm -rf "$CARGO_TARGET_DIR"
  era "END raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
  release_locks
  touch "$RUN/DONE"
  exit "${1:-0}"
}

# ── builds: release-prof (measured) and release (the .text reference) ───
textsha() { # sha of the .text section alone
  objcopy -O binary --only-section=.text "$1" "$ROOT/text.bin" 2>/dev/null || return 0
  sha256sum "$ROOT/text.bin" | cut -c1-16; rm -f "$ROOT/text.bin"
}
command -v objcopy >/dev/null 2>&1 || { era "ABORT-TEXT no objcopy"; finish 1; }
for prof in release-prof release; do
  t0=$(date +%s)
  (cd "$SRC" && "$CARGO" build --profile "$prof" --bin raptorpath) > "$RUN/build-$prof.log" 2>&1
  rc=$?
  era "BUILD $prof rc=$rc wall=$(( $(date +%s) - t0 ))s $(cotenants)"
  [ "$rc" = 0 ] || { era "ABORT-BUILD $prof"; finish 1; }
done
TP=$(textsha "$CARGO_TARGET_DIR/release-prof/raptorpath")
TR=$(textsha "$CARGO_TARGET_DIR/release/raptorpath")
era "TEXT release-prof=$TP release=$TR"
[ -n "$TP" ] && [ "$TP" = "$TR" ] || { era "ABORT-TEXT release-prof .text differs from release"; finish 1; }
cp "$CARGO_TARGET_DIR/release-prof/raptorpath" "$BIN"
sha256sum "$BIN" "$CARGO_TARGET_DIR/release/raptorpath" | tee "$RUN/BINSHA.txt"

# ── from here on, tmpfs (§22 Amendment 2) ───────────────────────────────
DISK_RUN="$RUN"
SHM="/dev/shm/r3prof-a2139"
rm -rf "$SHM"; mkdir -p "$SHM/diag" "$SHM/bin"
cp "$BIN" "$SHM/bin/raptorpath"; BIN="$SHM/bin/raptorpath"
cp "$RUN/era.txt" "$SHM/era.txt"; RUN="$SHM"; LB_LOG="$RUN/era.txt"
era "TMPFS run=$RUN bin_sha=$(sha256sum "$BIN" | cut -c1-16)"

# One recorded c1d transfer: tag bytes record_s. Leaves $RUN/<tag>.script.gz
# and the fold in $RUN/<tag>.fold.txt; sets FOLD_RX / FOLD_DEC.
profile() {
  local t="$1" bytes="$2" rs="$3" d srv comm cb ca prc nw P
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log "$RUN/aw.txt"
  cb=$(cotenants)
  sudo -n env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
      -u RWM_RTOBS -u RWM_RDIAG -u RWM_ACKDIAG -u RWM_IO_RT -u RWM_DIAG \
      RWM_GEN=0 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$BIN" SEED=42 RWM_DIAG=1 \
      AW_FILE="$RUN/aw.txt" \
    bash perf_rwm_c.sh c1 c1 bulk "$bytes" 1 dual > "$RUN/diag/$t-drv.out" 2>&1 &
  d=$!
  srv=""
  for _ in $(seq 1 300); do
    srv=$(sudo -n sed -n 's/^srv_pid=//p' "$RUN/aw.txt" 2>/dev/null | tail -1)
    [ -n "$srv" ] && grep -q -- '--- RWM-C perf' "$RUN/diag/$t-drv.out" 2>/dev/null && break
    sleep 0.1
  done
  sleep 3
  comm=$(cat "/proc/$srv/comm" 2>/dev/null)
  era "PERF $t srv_pid=${srv:-none} comm=${comm:-gone} record_s=$rs before[$cb]"
  P="$RUN/$t.perf.data"
  sudo -n perf record -g --call-graph dwarf,16384 -F 999 -p "${srv:-0}" -o "$P" -- sleep "$rs" > "$RUN/$t.rec.log" 2>&1
  prc=$?
  wait "$d"
  ca=$(cotenants)
  cp /tmp/rwm-c.log "$RUN/diag/$t-c.log" 2>/dev/null; cp /tmp/rwm-s.log "$RUN/diag/$t-s.log" 2>/dev/null
  era "RUN $t perf_rc=$prc $(grep -a -o '"mbps":[0-9.]*' "$RUN/diag/$t-c.log" | tail -1) after[$ca]"
  sudo -n perf report -i "$P" --sort comm --stdio > "$RUN/$t.comm.txt" 2>/dev/null
  nw=$(grep -c ' rp-w-' "$RUN/$t.comm.txt" 2>/dev/null)
  if [ "$prc" != 0 ] || [ "${nw:-0}" = 0 ]; then
    era "ABORT-PERF-ATTACH $t perf_rc=$prc rp_w_lines=${nw:-0}"
    finish 1
  fi
  era "PERF-ATTACH-OK $t rp_w_lines=$nw"
  sudo -n perf script -i "$P" -F tid,ip,sym --inline 2>/dev/null | gzip -c > "$RUN/$t.script.gz"
  zcat "$RUN/$t.script.gz" | python3 -I "$HERE/r3diag_fold.py" > "$RUN/$t.fold.txt" 2>&1
  FOLD_RX=$(sed -n 's/^samples=[0-9]* receiver=\([0-9]*\) .*/\1/p' "$RUN/$t.fold.txt")
  FOLD_NL=$(sed -n 's/^samples=.* receiver_nonleaf=\([0-9]*\).*/\1/p' "$RUN/$t.fold.txt")
  FOLD_DEC=$(sed -n 's/^-- under add_symbol: \([0-9]*\) samples.*/\1/p' "$RUN/$t.fold.txt")
  era "FOLD $t receiver=${FOLD_RX:-0} receiver_nonleaf=${FOLD_NL:-0} add_symbol=${FOLD_DEC:-0} $(head -1 "$RUN/$t.fold.txt")"
}

# Dry check: a short transfer, 5 s record; the fold must see run_receiver,
# and see it as a caller (samples with frames below it), so unwinding into
# its callees works.
profile D-c1d 400000000 5
sudo -n rm -f "$RUN/D-c1d.perf.data"
[ "${FOLD_RX:-0}" -gt 0 ] && [ "${FOLD_NL:-0}" -gt 0 ]   || { era "ABORT-FOLD-DRY receiver=${FOLD_RX:-0} receiver_nonleaf=${FOLD_NL:-0}"; finish 1; }

# The read: the pre-registered 1 GB c1d transfer, 3 s record.
profile P-c1d 1000000000 3
P="$RUN/P-c1d.perf.data"
sudo -n perf report -i "$P" --no-children --sort sym --stdio --percent-limit 0.3 > "$RUN/P-c1d.self.txt" 2>/dev/null
sudo -n perf report -i "$P" --children --sort sym --stdio --percent-limit 0.5 -g none > "$RUN/P-c1d.children.txt" 2>/dev/null
sudo -n rm -f "$P"
if [ "${FOLD_RX:-0}" -gt 0 ] && [ "${FOLD_DEC:-0}" -gt 0 ]; then
  era "FOLD-OK"
  finish 0
fi
era "ABORT-FOLD receiver=${FOLD_RX:-0} add_symbol=${FOLD_DEC:-0}"
finish 1
