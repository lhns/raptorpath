#!/bin/bash
# The full test suites on one tree, under both operator locks and the launch
# precondition (agent rules; status §26 amendment 1): release, doc, wasm,
# debug, as threadr2_run_all.sh step 1, plus the python parser tests.
#
#   SU_ROOT=/home/vibe/suites SU_SRC=<tree root> bash suites_run.sh
#
# Exit: 0 all green (TESTS-OK), 1 a REAL failure (TESTS-FAIL), 3 precondition
# not met, 4 a lock is held (nothing run; retry later). Started as `vibe`.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
ROOT="${SU_ROOT:?SU_ROOT}"
SRC="${SU_SRC:-$(cd "$HERE/../../.." && pwd)}"
mkdir -p "$ROOT"
OUT="$ROOT/TESTS.txt"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$ROOT/era.txt"; }
io_full() { awk -v k="$1" '/^full/{for(i=2;i<=NF;i++){split($i,a,"=");if(a[1]==k)print a[2]}}' /proc/pressure/io 2>/dev/null; }
iostate() { echo "io_full_avg10=$(io_full avg10) io_full_avg60=$(io_full avg60) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

# §26 amendment 3 (agent rules): builds and suites have no load or io gate;
# only the locks and no other cargo/rustc. The io state is recorded.
if [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
  era "ABORT-PRECONDITION cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) $(iostate); NOTHING WAS RUN."
  exit 3
fi
LB_TAG="suites_run:$$"
LB_LOG="$ROOT/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
trap release_locks EXIT
era "SUITES start src=$SRC commit=$(cat "$SRC/COMMIT" 2>/dev/null) $(iostate)"

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
export CARGO_TARGET_DIR="$ROOT/tgt"
rm -rf "$CARGO_TARGET_DIR"
: > "$OUT"
tst() { # name -- cmd...   (the §8/§11/§12 flake rule: a failure that passes on
        # an immediate solo re-run is FLAKE; any other is REAL)
  local name="$1"; shift
  local t0; t0=$(date +%s)
  local pre; pre=$(iostate)
  ( cd "$SRC" && timeout 5400 "$@" ) > "$ROOT/test-$name.log" 2>&1
  local rc=$?
  local sum
  sum=$(grep -a '^test result:' "$ROOT/test-$name.log" | awk '{p+=$4; f+=$6; i+=$8} END{printf "passed=%d failed=%d ignored=%d binaries=%d", p, f, i, NR}')
  echo "TEST $name rc=$rc wall=$(( $(date +%s) - t0 ))s $sum before[$pre] after[$(iostate)] cmd=[$*]" >> "$OUT"
  grep -a -E '^test .* FAILED$|^---- ' "$ROOT/test-$name.log" | head -40 >> "$OUT"
  era "TEST $name rc=$rc $sum"
  [ "$rc" = "0" ] && return 0
  if ! grep -aq -E '^test .* FAILED$' "$ROOT/test-$name.log"; then
    echo "REAL-FAILURE $name rc=$rc with no failing test named (build error or timeout?)" >> "$OUT"
    return 1
  fi
  local real=0 t a seen
  for t in $(grep -a -E '^test .* FAILED$' "$ROOT/test-$name.log" | awk '{print $2}' | sort -u); do
    local cmd=()
    seen=0
    for a in "$@"; do [ "$a" = "--" ] && seen=1; [ "$seen" = "0" ] && cmd+=("$a"); done
    if ( cd "$SRC" && "${cmd[@]}" -- "$t" --exact --test-threads=1 ) > "$ROOT/test-$name-rerun.log" 2>&1 \
        && grep -aq "^test $t ... ok" "$ROOT/test-$name-rerun.log"; then
      echo "FLAKE $name $t (passed on the immediate solo re-run)" >> "$OUT"
    else
      echo "REAL-FAILURE $name $t" >> "$OUT"
      real=1
    fi
  done
  return $real
}
TFAIL=0
tst release "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
tst doc "$CARGO" test --doc -p raptorpath --release || TFAIL=1
tst wasm env -u GOLDEN_CAPTURE "$CARGO" test -p raptorpath-wasm || TFAIL=1
tst debug "$CARGO" test -p raptorpath --no-fail-fast -- --test-threads=2 || TFAIL=1
tst parsers bash -c "cd raptorpath/tools/l1 && python3 test_l1common.py && python3 test_lagdiag_digest.py && python3 test_lagdiag2_digest.py" || TFAIL=1
echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$ROOT/test-debug.log")" >> "$OUT"
rm -rf "$CARGO_TARGET_DIR"
if [ "$TFAIL" = "0" ]; then touch "$ROOT/TESTS-OK"; era "SUITES OK"; exit 0; fi
touch "$ROOT/TESTS-FAIL"; era "SUITES FAIL (TESTS.txt)"; exit 1
