#!/bin/bash
# The abort-cause witness's own gate — `bash test_abort_witness.sh`.
#
# `abort_witness.sh` promises that it changes no harness behaviour: exit codes
# are preserved and `set -e` semantics at the call sites are unchanged. The
# failure this guards against: `aw_drain_probe` written as a bare
#
#     n=$(pgrep -x raptorpath 2>/dev/null | wc -l | tr -d ' ')
#
# under the caller's `set -euo pipefail` (`perf_rwm_c.sh` sources `lib.sh`).
# `pgrep` exits 1 when nothing matches — the normal case, since the caller has
# just `pkill`ed — `pipefail` carries that 1 past `wc`'s 0, and `set -e` kills
# the caller on the healthy path, leaving a truncated record with
# `abort_cause=none`.
#
# Prose asserting `set -e` safety is not `set -e` safety
# (docs/measurement-discipline.md rule 1). So every case below runs the
# function under `set -euo pipefail`, exactly as `perf_rwm_c.sh` does, and
# asserts the caller survived — which no assertion about the record's
# contents would catch, because a dead caller writes a well-formed truncated
# record.
#
# No root, no VM, no netns: it runs anywhere bash and pgrep do.
set -uo pipefail
cd "$(dirname "$0")"

PASS=0; FAIL=0
ok()   { PASS=$((PASS + 1)); echo "  ok   $*"; }
bad()  { FAIL=$((FAIL + 1)); echo "  FAIL $*"; }
check() { # description expected actual
    if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$2', got '$3')"; fi
}

TD="$(mktemp -d)"
trap 'rm -rf "$TD"; pkill -x raptorpath 2>/dev/null || true' EXIT

# The call site's regime, reproduced rather than described.
#
# The obvious harness — a subshell invoked as `run_probed && rc=0 || rc=$?` —
# passes against the buggy witness: `set -e` is suppressed for the whole of a
# command whose exit status is being tested, and the suppression is inherited
# by the function body and by any subshell inside it. Capturing the status
# that way disarms the mechanism under test.
#
# So the probe runs in a separate bash process, invoked as a plain command with
# its status read afterwards from `$?` — never from an `&&`/`||` list, never
# from an `if`. That is the context `perf_rwm_c.sh` calls `aw_drain_probe` in,
# and the only one in which the fault reproduces. Checked against a reverted
# `aw_drain_probe`: this gate must fail there, and it does.
write_probe() { # -> $TD/probe.sh
    cat > "$TD/probe.sh" <<PROBE
set -euo pipefail
export AW_FILE="$TD/rec.txt"
cd "$PWD"
source ./abort_witness.sh
aw_begin "test"
aw_drain_probe
# Unreachable if \`set -e\` fired inside the probe — the production symptom
# exactly: a truncated record, and a caller that never came back.
echo SURVIVED >> "\$AW_FILE"
PROBE
}
write_probe

echo "== 1. aw_drain_probe on an IDLE box (pgrep exits 1 — the healthy path)"
pkill -x raptorpath 2>/dev/null || true
rm -f "$TD/rec.txt"
bash "$TD/probe.sh" >/dev/null 2>&1
RC=$?
check "the caller survives the probe" 0 "$RC"
check "and reaches the line after it" "SURVIVED" \
    "$(grep -c '^SURVIVED$' "$TD/rec.txt" 2>/dev/null | sed 's/^1$/SURVIVED/')"
check "drain_pids_t0 is recorded as 0" "drain_pids_t0=0" \
    "$(grep '^drain_pids_t0=' "$TD/rec.txt" 2>/dev/null || echo MISSING)"

echo "== 2. aw_drain_probe WITH a survivor — the arm-correlation case itself"
# A real process named exactly `raptorpath`, because `pgrep -x` matches `comm`
# and the branch under test is the one that only runs when the match is
# non-empty. A `set -e` fault here would fire on exactly the invocations the
# drain column exists to explain — and would be invisible to case 1.
cp "$(command -v sleep)" "$TD/raptorpath"
"$TD/raptorpath" 30 &
FAKE=$!
sleep 0.3
rm -f "$TD/rec.txt"
bash "$TD/probe.sh" >/dev/null 2>&1
RC=$?
check "the caller survives with a survivor present" 0 "$RC"
check "and reaches the line after it" "SURVIVED" \
    "$(grep -c '^SURVIVED$' "$TD/rec.txt" 2>/dev/null | sed 's/^1$/SURVIVED/')"
N="$(grep '^drain_pids_t0=' "$TD/rec.txt" 2>/dev/null | cut -d= -f2)"
if [ "${N:-0}" -ge 1 ]; then ok "drain_pids_t0 counts the survivor (=$N)"
else bad "drain_pids_t0 counts the survivor (got '${N:-}')"; fi
check "the survivor's state is captured" 1 \
    "$(grep -c '^drain_states_t0=' "$TD/rec.txt" 2>/dev/null || echo 0)"
kill "$FAKE" 2>/dev/null || true
wait "$FAKE" 2>/dev/null || true

echo "== 3. FIRST WRITE WINS — the attribution rule, not decoration"
# A last-write-wins witness attributes every abort to the last step that ran,
# which for this harness is always `cli_exec`. The whole abort table depends on
# this holding.
rm -f "$TD/rec.txt"
(
    set -euo pipefail
    export AW_FILE="$TD/rec.txt"
    source ./abort_witness.sh
    aw_begin "test"
    aw_cause busy_precheck "the real cause"
    aw_cause cli_exec "the downstream consequence"
) >/dev/null 2>&1
check "the FIRST cause is the recorded one" "abort_cause=busy_precheck" \
    "$(grep '^abort_cause=' "$TD/rec.txt" 2>/dev/null || echo MISSING)"
check "exactly one abort_cause line" 1 \
    "$(grep -c '^abort_cause=' "$TD/rec.txt" 2>/dev/null || echo 0)"
check "the consequence is kept, not scored" 1 \
    "$(grep -c '^abort_also=' "$TD/rec.txt" 2>/dev/null || echo 0)"

echo "== 4. the reader's two absent-record verdicts stay distinct"
check "a missing record reads no_record" "no_record" \
    "$(python3 -c "import sys; sys.path.insert(0,'.'); from abort_witness import cause_or; print(cause_or('$TD/nope.txt'))")"
check "a record with no cause reads none" "none" \
    "$(python3 -c "
import sys; sys.path.insert(0,'.')
from abort_witness import cause_or
open('$TD/empty.txt','w').write('aw_version=1\n')
print(cause_or('$TD/empty.txt'))")"

echo
echo "abort-witness gate: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
