#!/bin/bash
# THE ABORT-CAUSE WITNESS'S OWN GATE — `bash test_abort_witness.sh`.
#
# WHY THIS FILE EXISTS, and it is not a hypothetical. `abort_witness.sh`'s
# header promises, in bold, that "NOTHING HERE CHANGES HARNESS BEHAVIOUR …
# exit codes are preserved and re-returned, `set -e` semantics at the call
# sites are unchanged … a witness that alters the thing it witnesses cannot
# clear a selection effect, it can only move it."
#
# IT DID EXACTLY THAT. `aw_drain_probe` opened with a bare
#
#     n=$(pgrep -x raptorpath 2>/dev/null | wc -l | tr -d ' ')
#
# and `perf_rwm_c.sh` sources `lib.sh`, which runs `set -euo pipefail`. `pgrep`
# exits 1 when nothing matches — the NORMAL case, since the caller has just
# `pkill`ed and is about to assert the box is idle — `pipefail` carries that 1
# past `wc`'s 0, and `set -e` killed the caller ON THE HEALTHY PATH. The era
# battery's smoke measured 6/6 invocations dead in under a second with
# `abort_cause=none` and a witness record that stopped at `bin=`: the recorder
# aborting the invocation it was recording, and then reporting no cause for it.
#
# The MEASUREMENT DISCIPLINE 1 reading of that failure is the reason for this
# file rather than for a careful re-read of the diff: the witness was deployed
# into three drivers and never once EXECUTED under the `set -e` regime of its
# own call site. Prose asserting `set -e` safety is not `set -e` safety. So
# every case below RUNS the function under `set -euo pipefail`, exactly as
# `perf_rwm_c.sh` does, and asserts the CALLER SURVIVED — which is the property
# that was actually broken, and which no assertion about the record's contents
# would have caught, because a dead caller writes a perfectly well-formed
# truncated record.
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

# The call site's regime, reproduced rather than described — AND THE FIRST DRAFT
# OF THIS FILE GOT IT WRONG IN A WAY WORTH KEEPING ON THE RECORD, because it is
# the same class of error as the bug.
#
# The obvious harness is a subshell invoked as `run_probed && rc=0 || rc=$?`.
# THAT HARNESS PASSES AGAINST THE BUGGY WITNESS. `set -e` is suppressed for the
# whole of a command whose exit status is being tested — and the suppression is
# INHERITED by the function body and by any subshell inside it. So the very act
# of capturing the status disarmed the mechanism under test, and the gate proved
# nothing while printing twelve `ok`s. (MEASUREMENT DISCIPLINE 1: prove the
# mechanism under test EXECUTES. A `set -e` test that reads its own subject's
# exit code with `||` has switched that subject off.)
#
# So the probe runs in a SEPARATE bash PROCESS, invoked as a plain command with
# its status read afterwards from `$?` — never from an `&&`/`||` list, never
# from an `if`. That is byte-for-byte the context `perf_rwm_c.sh` calls
# `aw_drain_probe` in, and it is the only context in which the fault reproduces.
# This file's own gate is `test_abort_witness.sh` run against a reverted
# `aw_drain_probe`: it MUST fail there, and it does.
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
# non-empty. This is the branch the c8/seed-7 class (20 % control vs 75 % RACK)
# is read off, so a `set -e` fault here would fire on exactly the invocations
# the column exists to explain — and would have been invisible to case 1.
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
