#!/bin/bash
# §20 (docs/status.md): the R1 c1s collapse diagnosis. Builds five arms
# (MAIN ce60079, R1 a5ef8b2, R1-d, MAIN+d, FIX), runs c1s-400 n = 2 per arm
# under RWM_RTOBS=2 RWM_DIAG=1 RWM_RDIAG=1, then server `perf record`s of a
# 1 GB c1s transfer (R1, MAIN, then R1-d). Both locks for the whole session.
#
#   R1D_ROOT=/home/vibe/r1diag R1D_HARD_DEADLINE=<epoch> \
#     setsid nohup bash r1diag_run.sh > $R1D_ROOT/launch.out 2>&1 < /dev/null &
#
# Layout: $ROOT/src-main, $ROOT/src-r1 (git archives), this script's tools
# tree, $ROOT/fix-lat-floor.patch. Started as `vibe`; `sudo -n` only to run.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
crlf_guard lib.sh lib_battery.sh perf_rwm_c.sh topo.sh topo_dual.sh r1diag_run.sh
ROOT="${R1D_ROOT:?R1D_ROOT}"
: "${R1D_HARD_DEADLINE:?R1D_HARD_DEADLINE (epoch seconds)}"
SOFT=$(( R1D_HARD_DEADLINE - 600 ))
RUN="$ROOT/run"
mkdir -p "$RUN/diag"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/era.txt"; }
sudo -n true 2>/dev/null || { era "ABORT-SUDO"; exit 3; }
LB_TAG="r1diag_run:$$"
LB_LOG="$RUN/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }
era "R1D start hard=$R1D_HARD_DEADLINE soft=$SOFT $(cotenants)"
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"

# ── arms: trees by file swap ────────────────────────────────────────────
LAT=raptorpath/src/net/lat.rs
mk_tree() { # arm base [swap-from-tree | patch]
  local arm="$1" base="$2" how="${3:-}"
  local t="$ROOT/tree-$arm"
  rm -rf "$t"; cp -a "$ROOT/$base" "$t"
  case "$how" in
    "") ;;
    *.patch) (cd "$t" && patch -p1 --no-backup-if-mismatch < "$how") >> "$RUN/era.txt" 2>&1 || return 1 ;;
    *) cp "$ROOT/$how/$LAT" "$t/$LAT" ;;
  esac
  era "TREE $arm base=$base swap=${how:-none} lat_sha=$(sha256sum "$t/$LAT" | cut -c1-16)"
}
build() { # arm
  local arm="$1" t0 rc
  t0=$(date +%s)
  (cd "$ROOT/tree-$arm" && CARGO_TARGET_DIR="$ROOT/tgt-$arm" "$CARGO" build --release --bin raptorpath) \
    > "$RUN/build-$arm.log" 2>&1
  rc=$?
  era "BUILD $arm rc=$rc wall=$(( $(date +%s) - t0 ))s $(cotenants)"
  [ "$rc" = 0 ] || return 1
  mkdir -p "$ROOT/bin/$arm"
  cp "$ROOT/tgt-$arm/release/raptorpath" "$ROOT/bin/$arm/raptorpath"
  rm -rf "$ROOT/tgt-$arm"
  echo "$arm $(sha256sum "$ROOT/bin/$arm/raptorpath" | cut -d' ' -f1)" >> "$RUN/BINSHA.txt"
}
: > "$RUN/BINSHA.txt"
ARMS=""
mk_tree MAIN src-main && build MAIN && ARMS="$ARMS MAIN"
mk_tree R1 src-r1 && build R1 && ARMS="$ARMS R1"
mk_tree R1-d src-r1 src-main && build R1-d && ARMS="$ARMS R1-d"
mk_tree MAIN+d src-main src-r1 && build MAIN+d && ARMS="$ARMS MAIN+d"
# FIX only if a build (prior: the slowest so far) plus the runs fit before soft.
SLOW=$(grep -o 'wall=[0-9]*' "$RUN/era.txt" | cut -d= -f2 | sort -n | tail -1)
if [ $(( $(date +%s) + ${SLOW:-1800} + 900 )) -lt "$SOFT" ]; then
  mk_tree FIX src-r1 "$ROOT/fix-lat-floor.patch" && build FIX && ARMS="$ARMS FIX"
else
  era "SKIP FIX (budget)"
fi
era "ARMS$ARMS"
rm -rf "$ROOT"/tree-*

# ── the diagnostic rows ──────────────────────────────────────────────────
drv() { # bin seed bytes extra-env... -> /tmp/r1d-drv.out
  local bin="$1" seed="$2" bytes="$3"
  shift 3
  sudo -n env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
      -u RWM_RTOBS -u RWM_RDIAG -u RWM_ACKDIAG -u RWM_IO_RT -u RWM_DIAG \
      RWM_GEN=0 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$bin" SEED="$seed" RWM_DIAG=1 "$@" \
    bash perf_rwm_c.sh c1 c1 bulk "$bytes" 1 single
}
row() { # arm seed rep
  local arm="$1" seed="$2" rep="$3" bin t
  bin="$ROOT/bin/$arm/raptorpath"
  t="c1s-400-$arm-s$seed-r$rep"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  local cb; cb=$(cotenants)
  drv "$bin" "$seed" 400000000 RWM_RTOBS=2 RWM_RDIAG=1 > "$RUN/diag/$t-drv.out" 2>&1
  local rc=$?
  cp /tmp/rwm-c.log "$RUN/diag/$t-c.log" 2>/dev/null; cp /tmp/rwm-s.log "$RUN/diag/$t-s.log" 2>/dev/null
  era "ROW $t rc=$rc sha=$(sha256sum "$bin" | cut -c1-16) $(grep -a -o '"mbps":[0-9.]*' "$RUN/diag/$t-c.log" | tail -1) before[$cb] after[$(cotenants)]"
}
ARR=($ARMS)
n=${#ARR[@]}
blk=0
for rep in 1 2; do
  seed=42; [ "$rep" = 2 ] && seed=7
  for ((j = 0; j < n; j++)); do
    [ "$(date +%s)" -ge "$SOFT" ] && { era "TRUNCATED rows"; break 2; }
    row "${ARR[$(( (j + blk) % n ))]}" "$seed" "$rep"
  done
  blk=$(( blk + 1 ))
done

# ── profiles ─────────────────────────────────────────────────────────────
prof() { # arm
  local arm="$1" bin t d srv
  bin="$ROOT/bin/$arm/raptorpath"; t="P-c1s-$arm"
  [ -x "$bin" ] || return 0
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  drv "$bin" 42 1000000000 RWM_RTOBS=1 > "$RUN/diag/$t-drv.out" 2>&1 &
  d=$!
  for _ in $(seq 1 300); do grep -q -- '--- RWM-C perf' "$RUN/diag/$t-drv.out" 2>/dev/null && break; sleep 0.1; done
  srv=$(pgrep -o -x raptorpath)
  sleep 3
  era "PERF $t srv=$srv"
  sudo -n perf record -g --call-graph dwarf,16384 -F 999 -p "$srv" -o "$RUN/$t.perf.data" -- sleep 3 > "$RUN/$t.rec.log" 2>&1
  wait "$d"
  cp /tmp/rwm-c.log "$RUN/diag/$t-c.log" 2>/dev/null; cp /tmp/rwm-s.log "$RUN/diag/$t-s.log" 2>/dev/null
  era "RUN $t $(grep -a -o '"mbps":[0-9.]*' "$RUN/diag/$t-c.log" | tail -1)"
  sudo -n perf report -i "$RUN/$t.perf.data" --no-children --sort sym --stdio --percent-limit 0.5 > "$RUN/$t.self.txt" 2>/dev/null
  sudo -n perf report -i "$RUN/$t.perf.data" --children --sort sym --stdio --percent-limit 1.5 -g none > "$RUN/$t.children.txt" 2>/dev/null
  sudo -n rm -f "$RUN/$t.perf.data"
}
if command -v perf >/dev/null 2>&1; then
  for a in R1 MAIN R1-d; do
    [ "$(date +%s)" -ge "$SOFT" ] && { era "SKIP PERF $a (budget)"; continue; }
    prof "$a"
  done
else
  era "NO-PERF"
fi
sudo -n pkill -x raptorpath 2>/dev/null || true
sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null
era "END raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
release_locks
touch "$RUN/DONE"
