#!/bin/bash
# Plan 67, session 0 (LD_PLAN=67pre; registration 2.2 and 6.0): the PRE-FLIGHT, no battery and no build. SOURCED by loopdiag_p67.sh after the seat commit pins (ABORT-SEAT); it never returns (every path exits).
# `67p` is plan 67's R2 battery plan (plan67.STAGE_PLAN), so session 0 has its own token (status 60's 60-pre). run_all holds both operator locks for the whole session (its budget guard, plan67.BUDGET_S["67pre"] = 17 040 s, has run).
# Seats B0, S1, S2 (LD_DROP_S2=1: no S2 step; the registration's guard 2 stops, guard 1 runs alone). Each step below gets fresh target dirs under $ROOT (they must not exist beforehand; removed after the step):
#   1. cargo check --all-targets on S1 and S2                          ABORT-PREFLIGHT plan 67pre seat <S>: cargo check --all-targets failed
#   2. cargo test -p raptorpath --lib gates on B0 (the reference)      $RUN/pre/GATES-B0.txt; no `test result` line = ABORT-PREFLIGHT seat B0 (the reference did not run); failures on B0 are GATES-B0-FINDING (a finding on main)
#   3. the full suites on S1, then S2 (run_suites: ADR-0080's rule; no tools tests: they run in session 1), $RUN/pre/suites-<S>/
#      F = the tests on REAL-FAILURE / FLAKE-FAILURE lines of TESTS.txt (QUARANTINED is not a failure); G = F on gates::*, N = F less G.
#      G not within B0's failing gates: ABORT-PREFLIGHT ... gates failure not equal on B0; N: ABORT-TESTS (both: both lines, ABORT-PREFLIGHT first); G within B0's: GATES-FINDING-ON-MAIN, the seat's suites count as passed
#   4. repeat67_run.sh (N = 30, its own ABORT-REPEAT / REPEAT-CUT / 60 min rule) on S1 and S2           ABORT-REPEAT plan 67pre seat <S>
#   5. S2: build_visualizer.sh (fresh wasm, in a scratch copy of the tree: the build rewrites interactive-visualizer.html) and the native golden test without capture   ABORT-PREFLIGHT ... seat S2: visualizer | golden
#   6. binid67_run.sh on B0, S1, S2 (B0, S1 under LD_DROP_S2=1), the reference shas for registration 5.3     ABORT-PREFLIGHT plan 67pre binid: <reason>
#   7. the record: $RUN/REF-MANIFEST-<S1|S2>.sha256 by src_manifest67 (the function session 1 compares against) and $RUN/SESSION0.txt (the results and the LD_REF_MANIFEST_<S> lines sessions 1-3 need); LD-ALL-SESSION0-DONE.
# Every step runs under `timeout` at the time left to LD_HARD; a step that would start with under 600 s left, or is killed by it (rc 124), is ABORT-BUDGET plan 67pre step <name> and nothing later runs.
# The children (repeat67_run.sh, binid67_run.sh) take the locks themselves: LB_LOCK_PARENT=$$ lets them inherit run_all's (lib_battery.sh take_lock). The test seams (R67_T1_S, R67_N, R67_BUDGET_S, R67_SIM) reach them only under LD_TEST_SEAMS=1.

P67END=""      # an abort here comes after earlier steps have run (no "NOTHING WAS RUN"); no raptorpath row ever runs
P0DIR="$RUN/pre"; mkdir -p "$P0DIR"
P0SEATS=(B0 S1); [ "$LD_DROP_S2" = 1 ] || P0SEATS+=(S2)
p0_has() { [[ " ${P0SEATS[*]} " == *" $1 "* ]]; }
p0_src() { echo "$ROOT/seats/${P67TREE[$1]}/src"; }
p0_left() { echo $(( LD_HARD - $(date +%s) )); }
p0_start() { # step: a step starts only with 600 s or more left to LD_HARD
  local l; l="$(p0_left)"
  [ "$l" -ge 600 ] || fail_all "ABORT-BUDGET plan 67pre step $1: ${l}s left before LD_HARD (a step needs 600 s)"
  era "LD-ALL plan 67pre step $1 (${l}s to LD_HARD)"
}
p0_killed() { # step rc
  [ "$2" != 124 ] || fail_all "ABORT-BUDGET plan 67pre step $1: killed by the timeout at LD_HARD"
}
p0_fresh() { # dir
  [ ! -e "$1" ] || fail_all "ABORT-PREFLIGHT plan 67pre: $1 exists: a session-0 target / work dir must be fresh"
}
p0_note() { echo "$*" >> "$P0DIR/NOTES.txt"; era "SESSION0 $*"; }
: > "$P0DIR/NOTES.txt"
P0CHILD=(env)
[ "${LD_TEST_SEAMS:-}" = 1 ] || P0CHILD+=(-u R67_T1_S -u R67_N -u R67_BUDGET_S -u R67_SIM)
P0CHILD+=(LB_LOCK_PARENT="$$" RWM_VM_LOCK="$LOCK_VM" RWM_RP_LOCK="$LOCK_RP")
era "LD-ALL plan 67pre: session 0 on ${P0SEATS[*]}; no build, no battery, no raptorpath row"

# ── 1. cargo check --all-targets (S1, S2) ──
for _s in S1 S2; do
  p0_has "$_s" || continue
  p0_start "check-$_s"
  _tgt="$ROOT/target67pre-check-$_s"; p0_fresh "$_tgt"
  ( cd "$(p0_src "$_s")" && CARGO_TARGET_DIR="$_tgt" CARGO_BUILD_JOBS=4 timeout "$(p0_left)" "$CARGO" check --all-targets ) > "$ROOT/check-$_s.log" 2>&1
  _rc=$?; rm -rf "$_tgt"
  p0_killed "check-$_s" "$_rc"
  [ "$_rc" = 0 ] || seat_fail "$_s" "ABORT-PREFLIGHT plan 67pre seat $_s: cargo check --all-targets failed ($ROOT/check-$_s.log)"
  p0_note "CHECK seat=$_s ok"
done

# ── 2. the gates reference on B0 ──
p0_start gates-B0
_tgt="$ROOT/target67pre-gates-B0"; p0_fresh "$_tgt"
( cd "$(p0_src B0)" && CARGO_TARGET_DIR="$_tgt" CARGO_BUILD_JOBS=4 timeout "$(p0_left)" "$CARGO" test -p raptorpath --lib gates ) > "$ROOT/gates-B0.log" 2>&1
_rc=$?; rm -rf "$_tgt"
p0_killed gates-B0 "$_rc"
grep -aq '^test result' "$ROOT/gates-B0.log" || fail_all "ABORT-PREFLIGHT plan 67pre seat B0: the gates reference did not run (no test result line: $ROOT/gates-B0.log)"
grep -aE '^test .* \.\.\. FAILED$' "$ROOT/gates-B0.log" | awk '{print $2}' | grep -a '^gates::' | LC_ALL=C sort -u > "$P0DIR/GATES-B0.txt"
P0GB0="$(tr '\n' ' ' < "$P0DIR/GATES-B0.txt")"; P0GB0="${P0GB0% }"
if [ -n "$P0GB0" ]; then p0_note "GATES-B0-FINDING $P0GB0 (a finding on main, not an abort)"; else p0_note "GATES B0 ok (no failing gates:: test)"; fi

# ── 3. the full suites on S1, then S2 ──
for _s in S1 S2; do
  p0_has "$_s" || continue
  p0_start "suites-$_s"
  SD="$P0DIR/suites-$_s"; mkdir -p "$SD"
  _tt="$ROOT/target67pre-suites-$_s"; p0_fresh "$_tt"
  ENGINE_SUITES=1 SUITES_REPO="$(p0_src "$_s")" SUITES_TT="$_tt" SUITES_NO_TOOLS=1 SUITES_HARD="$LD_HARD" run_suites
  _rrc=$?; rm -rf "$_tt"
  grep -aqE '^TEST .* rc=124 ' "$SD/TESTS.txt" && fail_all "ABORT-BUDGET plan 67pre step suites-$_s: a suite command was killed by the timeout at LD_HARD ($SD/TESTS.txt)"
  # the failing tests: REAL-FAILURE / FLAKE-FAILURE lines (`<kind> <suite> <test>`; a suite with no test named reads <suite>:no-failing-test-named)
  _g=" "; _n=" "; _gx=" "
  while read -r _kind _suite _rest; do
    case "$_rest" in *"with no failing test named"*|*"suites not run"*) _t="${_suite%:}:no-failing-test-named" ;; *) _t="${_rest%% *}" ;; esac
    case "$_t" in
      gates::*) [[ "$_g" == *" $_t "* ]] || _g="$_g$_t "; grep -qxF -- "$_t" "$P0DIR/GATES-B0.txt" || { [[ "$_gx" == *" $_t "* ]] || _gx="$_gx$_t "; } ;;
      *) [[ "$_n" == *" $_t "* ]] || _n="$_n$_t " ;;
    esac
  done < <(grep -aE '^(REAL|FLAKE)-FAILURE ' "$SD/TESTS.txt")
  [ "$_rrc" = 0 ] || [ "$_g$_n" != "  " ] || _n=" suites-failed-unclassified($SD/TESTS.txt) "
  _g="$(echo $_g)"; _n="$(echo $_n)"; _gx="$(echo $_gx)"
  _pre=""; _tst=""
  [ -z "$_gx" ] || _pre="ABORT-PREFLIGHT plan 67pre seat $_s: gates failure not equal on B0: $_gx"
  [ -z "$_n" ] || _tst="ABORT-TESTS plan 67pre seat $_s: $_n (ADR-0080: REAL or non-quarantined FLAKE)"
  if [ -n "$_pre" ] && [ -n "$_tst" ]; then era "$_tst"; seat_fail "$_s" "$_pre"; fi
  [ -z "$_pre" ] || seat_fail "$_s" "$_pre"
  [ -z "$_tst" ] || seat_fail "$_s" "$_tst"
  if [ -n "$_g" ]; then p0_note "GATES-FINDING-ON-MAIN seat=$_s $_g"; era "GATES-FINDING-ON-MAIN seat=$_s $_g"; fi
  # the registered touched-integration-test list (registration 2.2) runs inside these suites: which targets the logs show (reported, no gate)
  if [ "$_s" = S2 ]; then _list="holeclass_reachability recovery_bench sim_nack_test place_arms_reachability s06_no_copy s06_reactive sim_shed_reach sim_lost_tail"; else _list="config::"; fi
  _seen=""; _miss=""
  for _t in $_list; do
    if grep -aqF -- "$_t" "$SD"/test-*.log 2>/dev/null; then _seen="$_seen $_t"; else _miss="$_miss $_t"; fi
  done
  p0_note "SUITES seat=$_s ok (rc=$_rrc, gates findings: ${_g:-none}; $SD/TESTS.txt)"
  p0_note "TARGETS seat=$_s seen=[${_seen# }] not-seen=[${_miss# }] (reported, no gate)"
done

# ── 4. the repeat runs ──
p0_start repeat
_tgt="$ROOT/target67pre-repeat"; p0_fresh "$_tgt"
P0REP=(S1); p0_has S2 && P0REP+=(S2)
"${P0CHILD[@]}" R67_ROOT="$P0DIR/repeat" R67_SEATS="$ROOT/seats" R67_TGT="$_tgt" timeout "$(p0_left)" bash ./repeat67_run.sh "${P0REP[@]}" > "$P0DIR/repeat.out" 2>&1
_rc=$?; rm -rf "$_tgt"
p0_killed repeat "$_rc"
if [ "$_rc" != 0 ]; then
  for _s in "${P0REP[@]}"; do
    grep -aq "^ABORT-REPEAT seat=$_s" "$P0DIR/repeat/REPEAT67.txt" 2>/dev/null && seat_fail "$_s" "ABORT-REPEAT plan 67pre seat $_s ($(grep -a "^ABORT-REPEAT seat=$_s" "$P0DIR/repeat/REPEAT67.txt" | head -n 1); $P0DIR/repeat/REPEAT67.txt)"
  done
  fail_all "ABORT-REPEAT plan 67pre: repeat67_run.sh rc=$_rc (a precondition or a lock, not a seat's failure: $P0DIR/repeat.out)"
fi
p0_note "REPEAT ok on ${P0REP[*]} ($P0DIR/repeat/REPEAT67.txt)"

# ── 5. S2: the visualizer (fresh wasm) and the native golden test without capture ──
if p0_has S2; then
  p0_start visualizer
  [ ! -e "$(p0_src S2)/raptorpath-visualizer/wasm" ] || seat_fail S2 "ABORT-PREFLIGHT plan 67pre seat S2: visualizer: $(p0_src S2)/raptorpath-visualizer/wasm exists (the wasm must be built fresh)"
  _vt="$ROOT/vis67pre-S2"; _tgt="$ROOT/target67pre-vis-S2"; p0_fresh "$_vt"; p0_fresh "$_tgt"
  cp -a "$(p0_src S2)" "$_vt" || fail_all "ABORT-PREFLIGHT plan 67pre seat S2: visualizer: cannot copy the tree to $_vt"
  ( cd "$_vt" && CARGO_TARGET_DIR="$_tgt" CARGO_BUILD_JOBS=4 timeout "$(p0_left)" bash raptorpath-visualizer/build_visualizer.sh ) > "$ROOT/visualizer-S2.log" 2>&1
  _rc=$?; rm -rf "$_tgt"
  if [ "$_rc" = 124 ]; then rm -rf "$_vt"; fi
  p0_killed visualizer "$_rc"
  [ "$_rc" = 0 ] || { rm -rf "$_vt"; seat_fail S2 "ABORT-PREFLIGHT plan 67pre seat S2: visualizer build failed ($ROOT/visualizer-S2.log)"; }
  p0_start golden
  _tgt="$ROOT/target67pre-golden-S2"; p0_fresh "$_tgt"
  ( cd "$_vt" && env -u GOLDEN_CAPTURE CARGO_TARGET_DIR="$_tgt" CARGO_BUILD_JOBS=4 timeout "$(p0_left)" "$CARGO" test -p raptorpath-wasm golden ) > "$ROOT/golden-S2.log" 2>&1
  _rc=$?; rm -rf "$_tgt" "$_vt"
  p0_killed golden "$_rc"
  [ "$_rc" = 0 ] || seat_fail S2 "ABORT-PREFLIGHT plan 67pre seat S2: golden test failed ($ROOT/golden-S2.log)"
  grep -aqE '^test result: ok\. [1-9][0-9]* passed' "$ROOT/golden-S2.log" || seat_fail S2 "ABORT-PREFLIGHT plan 67pre seat S2: golden test ran no test ($ROOT/golden-S2.log)"
  p0_note "VISUALIZER seat=S2 ok (build_visualizer.sh in a scratch copy, fresh wasm); GOLDEN seat=S2 ok (no GOLDEN_CAPTURE)"
fi

# ── 6. binid on B0, S1, S2 ──
p0_start binid
_bw="$ROOT/b67work"; p0_fresh "$_bw"
"${P0CHILD[@]}" B67_ROOT="$P0DIR/binid" B67_WORK="$_bw" B67_SEATS="$ROOT/seats" timeout "$(p0_left)" bash ./binid67_run.sh "${P0SEATS[@]}" > "$P0DIR/binid.out" 2>&1
_rc=$?; rm -rf "$_bw"
p0_killed binid "$_rc"
if [ "$_rc" != 0 ] || ! grep -aq '^BINID67 PASS' "$P0DIR/binid/BINID.txt" 2>/dev/null; then
  _why="$(grep -a '^BINID67 FAIL' "$P0DIR/binid/BINID.txt" 2>/dev/null | head -n 1)"
  fail_all "ABORT-PREFLIGHT plan 67pre binid: ${_why:-binid67_run.sh rc=$_rc ($P0DIR/binid.out)}"
fi
p0_note "BINID ok on ${P0SEATS[*]} ($P0DIR/binid/BINID.txt)"

# ── 7. the record: the reference manifests (the function section 5 of loopdiag_p67.sh compares against) and SESSION0.txt ──
for _s in S1 S2; do
  p0_has "$_s" || continue
  src_manifest67 "$(p0_src "$_s")" > "$RUN/REF-MANIFEST-$_s.sha256"
  [ -s "$RUN/REF-MANIFEST-$_s.sha256" ] || fail_all "ABORT-PREFLIGHT plan 67pre seat $_s: the reference manifest of $(p0_src "$_s") is empty"
done
{
  echo "SESSION0 plan 67pre $(date -u +%FT%TZ) LD_DROP_S2=$LD_DROP_S2 seats: ${P0SEATS[*]}"
  for _s in "${P0SEATS[@]}"; do echo "SEAT $_s commit=${P67PIN[$_s]} tree=$(p0_src "$_s")"; done
  cat "$P0DIR/NOTES.txt"
  grep -aE '^(REPEAT67 |REPEAT-CUT )' "$P0DIR/repeat/REPEAT67.txt"
  grep -aE '^(BINID |BINTEST |BINID67 )' "$P0DIR/binid/BINID.txt"
  echo "BINID-WORK $ROOT/b67work (the fixed build path: the binary embeds location data; the coordinator's post-merge builds use the same path, registration 5.3)"
  echo "# the environment sessions 1-3 need (the persisted copies under $ROOT/run):"
  for _s in S1 S2; do p0_has "$_s" && echo "LD_REF_MANIFEST_$_s=$ROOT/run/REF-MANIFEST-$_s.sha256"; done
} > "$RUN/SESSION0.txt"
cat "$RUN/SESSION0.txt"
touch "$RUN/DONE-ALL"
persist; echo "LD-ALL-SESSION0-DONE"; exit 0
