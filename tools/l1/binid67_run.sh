#!/bin/bash
# Plan 67, session 0 (registration 2.2 / 9.6): binid_run.sh on THREE seats (B0, S1, S2), the reference for section 5.3's binary-identity check after each merge.
#
#   B67_ROOT=<out dir> B67_WORK=<fixed build path, e.g. ~/s67b> B67_SEATS=<$LD_ROOT/seats> bash binid67_run.sh
#
# Each seat's source tree ($B67_SEATS/<B0|S1|S2>/src plus its COMMIT file) is packed into a tarball and built by binid_run.sh at the SAME path with a fresh target and the harness's exact command (from the crate dir;
# the binary embeds location data, status 58 Part B). The three crate-directory shas are recorded (BINID67 lines) for the second amendment. They must be pairwise different: S1 differs from B0 by the gauges and the
# tunnel guard, S2 from S1 by the deletion; equal shas mean two seats are one tree (BINID67 FAIL). binid_run.sh's own exit code (a build failed) is carried. After a merge the coordinator builds the merged main the
# same way and compares with the seat's line here (the shas must be equal, section 5.3).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
ROOT="${B67_ROOT:?B67_ROOT}"
WORK="${B67_WORK:?B67_WORK}"
SEATS_DIR="${B67_SEATS:?B67_SEATS}"
mkdir -p "$ROOT/tgz"
args=()
for seat in B0 S1 S2; do
  d="$SEATS_DIR/$seat"
  [ -d "$d/src" ] || { echo "BINID67 FAIL: no tree $d/src"; exit 3; }
  stage="$ROOT/tgz/$seat-tree"; rm -rf "$stage"; mkdir -p "$stage"
  cp -a "$d/src/." "$stage/" && cp "$d/COMMIT" "$stage/COMMIT" || { echo "BINID67 FAIL: cannot stage $seat"; exit 3; }
  tar -czf "$ROOT/tgz/$seat.tgz" -C "$stage" . || { echo "BINID67 FAIL: cannot pack $seat"; exit 3; }
  rm -rf "$stage"
  args+=("$seat=$ROOT/tgz/$seat.tgz")
done
BI_ROOT="$ROOT" BI_WORK="$WORK" bash ./binid_run.sh "${args[@]}"
rc=$?
OUT="$ROOT/BINID.txt"
b0=$(sed -n 's/^BINID B0 sha256=\([0-9a-f]*\).*/\1/p' "$OUT"); s1=$(sed -n 's/^BINID S1 sha256=\([0-9a-f]*\).*/\1/p' "$OUT"); s2=$(sed -n 's/^BINID S2 sha256=\([0-9a-f]*\).*/\1/p' "$OUT")
echo "BINID67 B0=${b0:--} S1=${s1:--} S2=${s2:--}" | tee -a "$OUT"
if [ -z "$b0" ] || [ -z "$s1" ] || [ -z "$s2" ]; then echo "BINID67 FAIL: a seat produced no sha (rc=$rc)" | tee -a "$OUT"; exit 1; fi
if [ "$b0" = "$s1" ] || [ "$s1" = "$s2" ] || [ "$b0" = "$s2" ]; then echo "BINID67 FAIL: two seats built to the same binary" | tee -a "$OUT"; exit 1; fi
[ "$rc" = 0 ] || { echo "BINID67 FAIL: binid_run.sh rc=$rc" | tee -a "$OUT"; exit 1; }
echo "BINID67 PASS" | tee -a "$OUT"
exit 0
