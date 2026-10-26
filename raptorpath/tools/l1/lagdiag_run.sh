#!/bin/bash
# The [LAG] tail diagnosis (docs/status.md, lagdiag pre-registration).
# Measurement only: 30 c8-100 invocations (RTOBS=2 deep, RTOBS=1 control),
# each with a 10 Hz per-CPU /proc/stat sampler; then lagdiag_digest.py scores
# the rows (are late [LAG] ticks inside long task polls, or VM/guest CPU?).
#
#   LGD_ROOT=/home/vibe/lagdiag LGD_HARD_DEADLINE=<epoch> \
#     setsid nohup bash lagdiag_run.sh > $LGD_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced tree $LGD_ROOT/src/raptorpath/tools/l1 (git archive of
# the DIAG commit; $LGD_ROOT/src/COMMIT names it). Started as `vibe`;
# `sudo -n` only to run.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
crlf_guard lib.sh lib_battery.sh perf_rwm_c.sh topo.sh topo_dual.sh lagdiag_run.sh lagdiag_digest.py
ROOT="${LGD_ROOT:?LGD_ROOT}"
: "${LGD_HARD_DEADLINE:?LGD_HARD_DEADLINE (epoch seconds)}"
SOFT=$(( LGD_HARD_DEADLINE - 600 ))
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
BIN="$ROOT/bin/raptorpath"
mkdir -p "$RUN/diag" "$ROOT/bin"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/era.txt"; }
sudo -n true 2>/dev/null || { era "ABORT-SUDO"; exit 3; }

# Launch precondition, BEFORE the locks (as threadr2_run_all.sh).
LOAD1=$(cut -d' ' -f1 /proc/loadavg)
if ! awk -v l="$LOAD1" 'BEGIN{exit !(l < 2.0)}' || [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
  era "ABORT-PRECONDITION load1=$LOAD1 cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc); NOTHING WAS RUN."
  exit 3
fi
LB_TAG="lagdiag_run:$$"
LB_LOG="$RUN/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }
era "LGD start commit=$(cat "$ROOT/src/COMMIT" 2>/dev/null) hard=$LGD_HARD_DEADLINE soft=$SOFT $(cotenants)"

# Backstop at the hard deadline: pkill -x raptorpath only, never -f.
FIN="$RUN/.finished"
rm -f "$FIN"
(
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$LGD_HARD_DEADLINE" ] && break
    sleep 30
  done
  echo "LGD BACKSTOP fired $(date -u +%FT%TZ)" >> "$RUN/era.txt"
  echo "TRUNCATED by the lagdiag backstop (budget cap, not a failure of the rows)" > "$RUN/TRUNCATED.txt"
  sudo -n pkill -x raptorpath 2>/dev/null || true
) &
BACKSTOP_PID=$!
finish() { touch "$FIN" 2>/dev/null; kill "$BACKSTOP_PID" 2>/dev/null; release_locks; }
trap finish EXIT
trap 'finish; exit 130' INT
trap 'finish; exit 143' TERM
fail() { era "$*"; touch "$RUN/FAILED"; sudo -n pkill -x raptorpath 2>/dev/null || true; exit 5; }

# ── build (fresh target) + tests ─────────────────────────────────────────
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
T0=$(date +%s)
TGT="$ROOT/tgt"
rm -rf "$TGT"
( cd "$SRC" && CARGO_TARGET_DIR="$TGT" "$CARGO" build --release --bin raptorpath ) > "$RUN/build.log" 2>&1 \
  || fail "ABORT-BUILD (see build.log)"
cp "$TGT/release/raptorpath" "$BIN" || fail "ABORT-BUILD copy"
BINSHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
echo "$BINSHA  raptorpath" > "$RUN/BINSHA.txt"
era "BUILD done wall=$(( $(date +%s) - T0 ))s sha256=$BINSHA"
( cd "$SRC" && CARGO_TARGET_DIR="$TGT" "$CARGO" test -p raptorpath --release --lib -- runtime_obs task_obs gates --test-threads=2 ) \
  > "$RUN/test.log" 2>&1
TRC=$?
era "TESTS cargo rc=$TRC $(grep -a '^test result:' "$RUN/test.log" | tail -1)"
rm -rf "$TGT"
python3 test_l1common.py > "$RUN/test-l1common.log" 2>&1
PRC1=$?
python3 test_lagdiag_digest.py > "$RUN/test-digest.log" 2>&1
PRC2=$?
era "TESTS python l1common rc=$PRC1 digest rc=$PRC2"
[ "$TRC" = 0 ] && [ "$PRC1" = 0 ] && [ "$PRC2" = 0 ] || fail "ABORT-TESTS (nothing measured)"

# ── one invocation ───────────────────────────────────────────────────────
statmon() { # out: 10 Hz epoch + per-CPU jiffies
  while :; do
    echo "T $(date +%s.%N)"
    grep '^cpu' /proc/stat
    sleep 0.1
  done > "$1"
}
cot_count() { echo $(( $(pgrep -xc cargo) + $(pgrep -xc rustc) )); }
row() { # tag seed rtobs
  local tag="$1" seed="$2" rtobs="$3" sha cb ca rc mon base mbps
  [ "$(date +%s)" -lt "$SOFT" ] || { era "SKIP-DEADLINE $tag"; return 1; }
  sha="$(sha256sum "$BIN" | cut -d' ' -f1)"
  [ "$sha" = "$BINSHA" ] || fail "ABORT-SHA $tag: $sha != $BINSHA"
  base="$RUN/diag/$tag"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/lgd-drv.out
  cb=$(cot_count)
  statmon "$base-stat.txt" &
  mon=$!
  sudo -n env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
      -u RWM_RTOBS -u RWM_RDIAG -u RWM_ACKDIAG -u RWM_IO_RT \
      RWM_RTOBS="$rtobs" RWM_RDIAG=1 SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
      RWM_C_PIPELINE=window RWM_BIN="$BIN" \
    bash perf_rwm_c.sh c2 c3 bulk 100000000 1 dual > /tmp/lgd-drv.out 2>&1
  rc=$?
  kill "$mon" 2>/dev/null
  wait "$mon" 2>/dev/null
  ca=$(cot_count)
  cp /tmp/lgd-drv.out "$base-drv.out" 2>/dev/null
  cp /tmp/rwm-c.log "$base-c.log" 2>/dev/null
  cp /tmp/rwm-s.log "$base-s.log" 2>/dev/null
  mbps=$(grep -a -o '"mbps":[0-9.]*' "$base-c.log" 2>/dev/null | tail -1 | cut -d: -f2)
  era "ROW $tag rc=$rc mbps=${mbps:-} cot_before=$cb cot_after=$ca load=$(cut -d' ' -f1-3 /proc/loadavg)"
  return 0
}

# ── smoke ────────────────────────────────────────────────────────────────
row smoke-s42-rt2 42 2 || fail "ABORT-SMOKE (deadline)"
if python3 lagdiag_digest.py smoke "$RUN/diag/smoke-s42-rt2" > "$RUN/smoke-check.txt" 2>&1; then
  era "SMOKE-PASS"
else
  fail "ABORT-SMOKE (see smoke-check.txt)"
fi

# ── plan: 12 blocks, 30 rows ─────────────────────────────────────────────
for b in $(seq 1 12); do
  if [ $(( b % 2 )) = 1 ]; then first=42; second=7; else first=7; second=42; fi
  row "b$b-s$first-rt2" "$first" 2 || break
  row "b$b-s$second-rt2" "$second" 2 || break
  if [ $(( b % 2 )) = 0 ]; then
    case "$b" in 2|6|10) cs=42 ;; *) cs=7 ;; esac
    row "b$b-s$cs-rt1" "$cs" 1 || break
  fi
done

# ── end ──────────────────────────────────────────────────────────────────
python3 lagdiag_digest.py digest "$RUN/diag" > "$RUN/digest.txt" 2>&1
sudo -n pkill -x raptorpath 2>/dev/null || true
sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null
era "END raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') $(grep -a '^OUTCOME' "$RUN/digest.txt" | head -1)"
if [ -f "$RUN/TRUNCATED.txt" ]; then touch "$RUN/FAILED"; else touch "$RUN/DONE"; fi
