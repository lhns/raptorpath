#!/bin/bash
# Threading Q3 step 1, session 2 (docs/status.md §16): the receiver-queue
# check (RWM_RDIAG=1: the server receiver's busy share and its input queue
# depth in owner batches) at c1s / c1d, and per-process `perf record`s
# (server, then client) in the steady state of a 1 GB transfer. Reuses
# session 1's binary ($Q3_ROOT/bin/raptorpath). Both locks, as session 1.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
crlf_guard lib.sh lib_battery.sh perf_rwm_c.sh topo.sh topo_dual.sh q3diag_run2.sh
ROOT="${Q3_ROOT:?Q3_ROOT}"
RUN="$ROOT/run2"
BIN="$ROOT/bin/raptorpath"
mkdir -p "$RUN"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/era.txt"; }
sudo -n true 2>/dev/null || { era "ABORT-SUDO"; exit 3; }
[ -x "$BIN" ] || { era "ABORT-NOBIN"; exit 3; }
LB_TAG="q3diag_run2:$$"
LB_LOG="$RUN/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
era "start sha256=$(sha256sum "$BIN" | cut -d' ' -f1)"

drv() { # cell bytes extra-env...
  local cell="$1" bytes="$2" ca cb mode
  shift 2
  case "$cell" in
    c1s) ca=c1; cb=c1; mode=single ;;
    c1d) ca=c1; cb=c1; mode=dual ;;
  esac
  sudo -n env -u RWM_DIAG -u RWM_RTOBS -u RWM_RDIAG -u RWM_EMIT_BATCH -u RWM_EMIT_BURST -u RWM_IO_RT \
    RWM_GEN=0 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$BIN" SEED=42 RWM_DIAG=1 "$@" \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode"
}

for r in 1 2; do
  for cell in c1s c1d; do
    t="R-$cell-r$r"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    drv "$cell" 400000000 RWM_RTOBS=1 RWM_RDIAG=1 > "$RUN/$t-drv.out" 2>&1
    cp /tmp/rwm-c.log "$RUN/$t-c.log"; cp /tmp/rwm-s.log "$RUN/$t-s.log"
    era "RUN $t $(grep -a -o '"mbps":[0-9.]*' "$RUN/$t-c.log" | tail -1)"
  done
done

for cell in c1s c1d; do
  t="P-$cell"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  drv "$cell" 1000000000 RWM_RTOBS=1 > "$RUN/$t-drv.out" 2>&1 &
  d=$!
  for _ in $(seq 1 300); do grep -q -- '--- RWM-C perf' "$RUN/$t-drv.out" 2>/dev/null && break; sleep 0.1; done
  srv=$(pgrep -o -x raptorpath)
  sleep 3
  cli=$(pgrep -n -x raptorpath)
  era "PERF $t srv=$srv cli=$cli"
  sudo -n perf record -g --call-graph dwarf,16384 -F 999 -p "$srv" -o "$RUN/$t-srv.perf.data" -- sleep 3 > "$RUN/$t-srv.rec.log" 2>&1
  sudo -n perf record -g --call-graph dwarf,16384 -F 999 -p "$cli" -o "$RUN/$t-cli.perf.data" -- sleep 2.5 > "$RUN/$t-cli.rec.log" 2>&1
  wait "$d"
  cp /tmp/rwm-c.log "$RUN/$t-c.log"; cp /tmp/rwm-s.log "$RUN/$t-s.log"
  era "RUN $t $(grep -a -o '"mbps":[0-9.]*' "$RUN/$t-c.log" | tail -1)"
done

for f in "$RUN"/*.perf.data; do
  b="${f%.perf.data}"
  sudo -n perf report -i "$f" --no-children --sort sym --stdio --percent-limit 0.5 > "$b.self.txt" 2>/dev/null
  sudo -n perf report -i "$f" --children --sort sym --stdio --percent-limit 1.5 -g none > "$b.children.txt" 2>/dev/null
  sudo -n rm -f "$f"
done
sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null
era "END raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
release_locks
touch "$RUN/DONE"
