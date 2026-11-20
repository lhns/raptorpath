#!/bin/bash
# Run the tier-A test set N times (default 30) and list every test that failed
# at least once, with its failure count and per-test pass counts. Builds once.
# The flake rule (ADR-0080, tools/ci/flake_rule.py): one failure in N is a
# failure; the job exits 1 unless every failing test is on
# raptorpath/tests/quarantine.txt (tools/l1 tests: tools/l1/quarantine.txt,
# which run_py_tests.py already skips).
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
: > "$OUT/failures.txt"; : > "$OUT/passes.txt"
for c in "${CMDS[@]}"; do $c --no-run || echo "BUILD-FAILED [$c]" >> "$OUT/failures.txt"; done
for i in $(seq 1 "$N"); do
  for c in "${CMDS[@]}"; do
    $c --no-fail-fast > "$OUT/run.log" 2>&1
    grep -aE '^test .* \.\.\. (ok|FAILED)$' "$OUT/run.log" | awk -v c="$c" '{print $2 " [" c "] " $4}' >> "$OUT/passes.txt"
    grep -aE '^test .* FAILED$' "$OUT/run.log" | awk -v c="$c" '{print $2 " [" c "]"}' >> "$OUT/failures.txt"
    if ! grep -aq '^test result' "$OUT/run.log"; then echo "NO-RESULT [$c]" >> "$OUT/failures.txt"; fi
  done
  # the tools/l1 set takes ~4 min (one test is 130 s); PY_EVERY=k runs it on every k-th iteration
  if [ $(( (i - 1) % ${PY_EVERY:-1} )) -eq 0 ]; then
    python3 tools/ci/run_py_tests.py > "$OUT/py.log" 2>&1 || grep -aE '^FAIL  test_' "$OUT/py.log" | awk '{print $2 " [py]"}' >> "$OUT/failures.txt"
    grep -aE '^(ok   |FAIL ) test_' "$OUT/py.log" | awk '{print $2 " [py] " ($1 == "ok" ? "ok" : "FAILED")}' >> "$OUT/passes.txt"
  fi
  echo "iteration $i done"
done
{
  echo "## Repeat run: $N iterations of the tier-A set"
  echo '```'
  python3 tools/ci/flake_rule.py --failures "$OUT/failures.txt" --passes "$OUT/passes.txt" --runs "$N"
  rc=$?
  echo '```'
  echo "$rc" > "$OUT/rc.txt"
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
exit "$(cat "$OUT/rc.txt")"
