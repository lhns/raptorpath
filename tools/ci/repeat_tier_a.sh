#!/bin/bash
# Run the tier-A test set N times (default 30) and list every test that failed
# at least once, with its failure count. Builds once. Never fails the job: it
# seeds the quarantine list.
#   usage: bash tools/ci/repeat_tier_a.sh [N]
set -u
N="${1:-30}"
OUT="${REPEAT_OUT:-repeat-out}"; mkdir -p "$OUT"
CMDS=(
  "cargo test -p raptorpath --lib gates"
  "cargo test -p raptorpath-math"
  "cargo test -p raptorpath-core"
  "cargo test -p raptorpath --lib --release"
)
for c in "${CMDS[@]}"; do $c --no-run || echo "BUILD FAILED: $c"; done
: > "$OUT/failures.txt"
for i in $(seq 1 "$N"); do
  for c in "${CMDS[@]}"; do
    $c --no-fail-fast > "$OUT/run.log" 2>&1
    grep -aE '^test .* FAILED$' "$OUT/run.log" | awk -v c="$c" '{print $2 " [" c "]"}' >> "$OUT/failures.txt"
    if ! grep -aq '^test result' "$OUT/run.log"; then echo "NO-RESULT [$c]" >> "$OUT/failures.txt"; fi
  done
  # the tools/l1 set takes ~4 min (mostly one 130 s test), so it runs on iterations 1, 11, 21 only
  if [ $(( (i - 1) % 10 )) -eq 0 ]; then
    python3 tools/ci/run_py_tests.py > "$OUT/py.log" 2>&1 || grep -aE '^FAIL  test_' "$OUT/py.log" | awk '{print $2 " [py]"}' >> "$OUT/failures.txt"
  fi
  echo "iteration $i done"
done
{
  echo "## Repeat run: $N iterations of the tier-A set"
  if [ -s "$OUT/failures.txt" ]; then
    echo "Tests that failed at least once (count = failures out of $N):"; echo '```'
    sort "$OUT/failures.txt" | uniq -c | sort -rn
    echo '```'
  else echo "No failures in $N iterations."; fi
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
exit 0
