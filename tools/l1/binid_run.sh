#!/bin/bash
# Build-identity check: build each given tree with the measurement harness's
# exact command, record the binary's sha256, then run the math+wasm tests on
# that tree. The same label may repeat (a base built twice shows whether the
# build is reproducible). Under both operator locks and the launch precondition.
#
#   BI_ROOT=<dir> BI_WORK=<fixed path, e.g. ~/s58b> bash binid_run.sh <label>=<tree.tgz> [...]
#
# Output: $BI_ROOT/BINID.txt (BINID / BINTEST lines, then BINID-DONE), logs
# $BI_ROOT/{build,test}-<n>-<label>.log.
# Exit: 0 every build succeeded, 1 a build failed, 3 precondition not met,
# 4 a lock is held (nothing run).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
ROOT="${BI_ROOT:?BI_ROOT}"
WORK="${BI_WORK:?BI_WORK}"
[ "$#" -gt 0 ] || { echo "usage: binid_run.sh <label>=<tree.tgz> ..." >&2; exit 3; }
mkdir -p "$ROOT" "$WORK"
OUT="$ROOT/BINID.txt"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$ROOT/era.txt"; }

if [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
  era "ABORT-PRECONDITION cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc); NOTHING WAS RUN."
  exit 3
fi
LB_TAG="binid_run:$$"
LB_LOG="$ROOT/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
trap release_locks EXIT
era "BINID start work=$WORK trees=$#"

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
: > "$OUT"
FAIL=0
n=0
for arg in "$@"; do
  n=$((n + 1))
  label="${arg%%=*}"
  tgz="${arg#*=}"
  rm -rf "$WORK/src" "$WORK/tgt"
  mkdir -p "$WORK/src"
  if ! tar -xzf - -C "$WORK/src" < "$tgz"; then
    echo "BINID $label BUILD-FAIL (unpack $tgz)" >> "$OUT"; era "BINID $label unpack-fail"; FAIL=1; continue
  fi
  blog="$ROOT/build-$n-$label.log"
  if ( cd "$WORK/src" && CARGO_TARGET_DIR="$WORK/tgt" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$blog" 2>&1 \
      && [ -f "$WORK/tgt/release/raptorpath" ]; then
    sha=$(sha256sum "$WORK/tgt/release/raptorpath" | cut -d' ' -f1)
    echo "BINID $label sha256=$sha commit=$(cat "$WORK/src/COMMIT" 2>/dev/null)" >> "$OUT"
    era "BINID $label sha256=$sha"
  else
    echo "BINID $label BUILD-FAIL ($blog)" >> "$OUT"; era "BINID $label BUILD-FAIL"; FAIL=1; continue
  fi
  tlog="$ROOT/test-$n-$label.log"
  ( cd "$WORK/src" && CARGO_TARGET_DIR="$WORK/tgt" timeout 5400 env -u GOLDEN_CAPTURE "$CARGO" test -p raptorpath-math -p raptorpath-wasm ) > "$tlog" 2>&1
  rc=$?
  sum=$(grep -a '^test result:' "$tlog" | awk '{p+=$4; f+=$6} END{printf "passed=%d failed=%d", p, f}')
  echo "BINTEST $label rc=$rc $sum" >> "$OUT"
  era "BINTEST $label rc=$rc $sum"
done
rm -rf "$WORK/src" "$WORK/tgt"
echo "BINID-DONE" >> "$OUT"
era "BINID-DONE fail=$FAIL"
exit "$FAIL"
