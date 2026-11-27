#!/bin/bash
# Plan 67, session 0 (registration 2.2): the REPEAT run of the gating loopback tests, N = 30, zero failures.
#
#   R67_ROOT=<out dir> R67_SEATS=<$LD_ROOT/seats> [R67_TGT=<dir for the targets>] [R67_N=30] [R67_BUDGET_S=3600] bash repeat67_run.sh [S1] [S2]
#
# Per seat (trees at $R67_SEATS/<S1|S2>/src, built with the harness's own commands; no seat is built or run twice in parallel):
#   S1: `cargo test -p raptorpath --release --lib -- config::` x N                                   (the tunnel guard's config tests)
#   S2: `cargo test -p raptorpath --release --features test-seams --test s06_no_copy --test s06_reactive --test holeclass_reachability --no-fail-fast` x N
#       (every non-ignored test of those three targets; s06_reactive requires test-seams, and cargo refuses a named target whose required features are off; T2 is a sim test and runs below)
#   sim tests (deterministic): once each seat's list (R67_SIM, default sim_shed_reach sim_lost_tail sim_s06_nack sim_nack_test place_arms_reachability) TWICE, the two result digests must be equal
#       (digest = the sorted `test <name> ... <ok|FAILED|ignored>` lines, timings stripped). A target whose [[test]] stanza requires `sim` (sim_shed_reach, sim_lost_tail, sim_s06_nack) runs
#       as ci.yml's sim job runs it: RUSTFLAGS="--cfg tokio_unstable", --features sim,test-seams, debug profile, in a target dir of its own (RUSTFLAGS would rebuild the repeat's);
#       the plain ones (sim_nack_test, place_arms_reachability) run --release with no feature, as before. sim_s06_nack is T2's deterministic port (registration 2.1 item 3).
# Every run must show a `Running` line for each named target and at least one passed test (ZERO-TESTS / TARGET-MISSING otherwise), so a target that did not run never reads as zero failures.
# Zero failures means zero: a test named FAILED, a run with no `test result` line, or a non-zero exit with no named test is a failure (no quarantine here: ADR-0080's rule applies to the suites, this is the
# flake detector). Any failure: `ABORT-REPEAT seat=<S>`, that seat's guard does not launch (S1: guard 1 and guard 2's BASE; S2: guard 2); exit 5. The budget (R67_BUDGET_S, registered 60 min for both seats)
# is split equally between the seats: each seat's N is cut to what the first timed run says fits, `REPEAT-CUT seat=<S> reached=<n>` is written (the cut goes into the second amendment, never silently). Exit 0 only if every seat read N
# (or its recorded cut) with zero failures. Locks and the launch precondition as suites_run.sh.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
ROOT="${R67_ROOT:?R67_ROOT}"
SEATS_DIR="${R67_SEATS:?R67_SEATS}"
N="${R67_N:-30}"
BUDGET="${R67_BUDGET_S:-3600}"
SIM="${R67_SIM:-sim_shed_reach sim_lost_tail sim_s06_nack sim_nack_test place_arms_reachability}"
TGT="${R67_TGT:-$ROOT}"      # where the per-seat target dirs go (plan 67pre passes a disk path: its R67_ROOT is on tmpfs)
[ "$#" -gt 0 ] || set -- S1 S2
mkdir -p "$ROOT"
OUT="$ROOT/REPEAT67.txt"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$ROOT/era.txt"; }
if [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
  era "ABORT-PRECONDITION cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc); NOTHING WAS RUN."
  exit 3
fi
LB_TAG="repeat67_run:$$"
LB_LOG="$ROOT/era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-$HOME/rp.lock}"
trap release_locks EXIT
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
: > "$OUT"
NSEATS=$#
BAD=0

digest() { grep -aE '^test .* \.\.\. (ok|FAILED|ignored)' "$1" | LC_ALL=C sort; }
# ran <log> <target...>: what is missing from a run that exited cleanly: TARGET-MISSING(<t>) per named target with no `Running` line, ZERO-TESTS when no `test result` line reports a passed test.
# needs_sim <Cargo.toml> <target>: the target's [[test]] stanza lists `sim` in required-features
needs_sim() {
  awk -v t="$2" '/^\[/ {inside = 0} /^name = / {inside = ($3 == "\"" t "\"")} inside && /^required-features/ && /"sim"/ {f = 1} END {exit !f}' "$1"
}
ran() {
  local log="$1" t re miss="" npass; shift
  for t in "$@"; do
    if [ "$t" = lib ]; then re='unittests src/lib\.rs'; else re="tests/$t\.rs"; fi
    grep -aqE "^ +Running $re" "$log" || miss="${miss}TARGET-MISSING($t) "
  done
  npass=$(grep -aoE '^test result: [A-Za-z]+\. [0-9]+ passed' "$log" | awk '{s += $4} END {print s + 0}')
  [ "$npass" -gt 0 ] || miss="${miss}ZERO-TESTS "
  echo "$miss"
}

for seat in "$@"; do
  src="$SEATS_DIR/$seat/src"
  [ -d "$src" ] || { echo "ABORT-REPEAT seat=$seat (no tree $src)" >> "$OUT"; era "ABORT-REPEAT seat=$seat no tree"; BAD=1; continue; }
  export CARGO_TARGET_DIR="$TGT/tgt-$seat"
  rm -rf "$CARGO_TARGET_DIR"
  case "$seat" in
    S1) cmd=("$CARGO" test -p raptorpath --release --lib) ; filt=(-- config::) ; sims="" ; tgts=(lib) ;;
    S2) cmd=("$CARGO" test -p raptorpath --release --features test-seams --test s06_no_copy --test s06_reactive --test holeclass_reachability --no-fail-fast) ; filt=() ; sims="$SIM"
        tgts=(s06_no_copy s06_reactive holeclass_reachability) ;;
    *) echo "ABORT-REPEAT seat=$seat (unknown seat)" >> "$OUT"; BAD=1; continue ;;
  esac
  ( cd "$src" && "${cmd[@]}" --no-run ) > "$ROOT/build-$seat.log" 2>&1 || { echo "ABORT-REPEAT seat=$seat (build: $ROOT/build-$seat.log)" >> "$OUT"; era "ABORT-REPEAT seat=$seat build"; BAD=1; continue; }
  # the repeat: the first run is timed and decides the cut; failures are counted per test
  fails=""; reached=0; nfail=0; t1=1; N_SEAT=$N
  left=$(( BUDGET / NSEATS ))      # each seat gets an equal share of the budget; the first timed run decides how many runs fit it
  for i in $(seq 1 "$N"); do
    t0=$(date +%s)
    ( cd "$src" && "${cmd[@]}" "${filt[@]}" ) > "$ROOT/run-$seat.log" 2>&1
    rc=$?
    dt=$(( $(date +%s) - t0 )); [ "$dt" -ge 1 ] || dt=1
    [ "$i" != 1 ] || [ -z "${R67_T1_S:-}" ] || dt="$R67_T1_S"      # test seam: the first run's duration
    reached=$i
    named="$(grep -aE '^test .* FAILED$' "$ROOT/run-$seat.log" | awk '{print $2}' | sort -u | tr '\n' ' ')"
    if [ -n "$named" ]; then fails="$fails$named"; nfail=$(( nfail + 1 ))
    elif ! grep -aq '^test result' "$ROOT/run-$seat.log" || [ "$rc" != 0 ]; then fails="${fails}NO-RESULT(rc=$rc) "; nfail=$(( nfail + 1 ))
    else
      miss="$(ran "$ROOT/run-$seat.log" "${tgts[@]}")"
      [ -z "$miss" ] || { fails="$fails$miss"; nfail=$(( nfail + 1 )); }
    fi
    if [ "$i" = 1 ]; then
      t1=$dt
      fit=$(( left / t1 ))
      if [ "$fit" -lt "$N" ]; then
        echo "REPEAT-CUT seat=$seat reached=$fit of $N (first run ${t1}s, ${left}s left of the ${BUDGET}s budget: write the cut into the second amendment)" >> "$OUT"
        era "REPEAT-CUT seat=$seat reached=$fit"
        [ "$fit" -ge 1 ] || { echo "ABORT-REPEAT seat=$seat (the budget does not fit one run)" >> "$OUT"; BAD=1; fit=0; break; }
        N_SEAT=$fit
      else N_SEAT=$N; fi
    fi
    [ "$i" -lt "${N_SEAT:-$N}" ] || break
  done
  # the sim tests: twice, equal digests
  simbad=""
  if [ -n "$sims" ] && [ "$reached" -ge 1 ]; then
    # split by the seat's own manifest: a target whose [[test]] stanza requires `sim` runs as ci.yml's sim job does; the others (plain integration tests) run --release with no feature, as before
    sargs=(); pargs=()
    for t in $sims; do if needs_sim "$src/raptorpath/Cargo.toml" "$t"; then sargs+=(--test "$t"); else pargs+=(--test "$t"); fi; done
    for k in 1 2; do
      : > "$ROOT/sim$k-$seat.log"
      if [ "${#sargs[@]}" -gt 0 ]; then
        ( cd "$src" && CARGO_TARGET_DIR="$CARGO_TARGET_DIR-sim" RUSTFLAGS="--cfg tokio_unstable" "$CARGO" test -p raptorpath --features sim,test-seams "${sargs[@]}" --no-fail-fast ) >> "$ROOT/sim$k-$seat.log" 2>&1
        src_rc=$?; [ "$src_rc" = 0 ] || simbad="${simbad}SIM-RC$k=$src_rc "
      fi
      if [ "${#pargs[@]}" -gt 0 ]; then
        ( cd "$src" && "$CARGO" test -p raptorpath --release "${pargs[@]}" --no-fail-fast ) >> "$ROOT/sim$k-$seat.log" 2>&1
        src_rc=$?; [ "$src_rc" = 0 ] || simbad="${simbad}DET-RC$k=$src_rc "
      fi
      grep -aq '^test result' "$ROOT/sim$k-$seat.log" || simbad="${simbad}SIM-NO-RESULT$k "
      miss="$(ran "$ROOT/sim$k-$seat.log" $sims)"
      [ -z "$miss" ] || simbad="${simbad}SIM$k:$miss"
    done
    [ "$(digest "$ROOT/sim1-$seat.log")" = "$(digest "$ROOT/sim2-$seat.log")" ] || simbad="${simbad}SIM-DIGEST-DIFFERS "
    grep -aqE '^test .* FAILED$' "$ROOT/sim1-$seat.log" "$ROOT/sim2-$seat.log" && simbad="${simbad}SIM-FAILED "
    rm -rf "$CARGO_TARGET_DIR-sim"
  fi
  echo "REPEAT67 seat=$seat reached=$reached of $N failing_runs=$nfail failures=[${fails% }] sim=[${simbad% }]" >> "$OUT"
  era "REPEAT67 seat=$seat reached=$reached failing_runs=$nfail sim=[${simbad% }]"
  if [ "$nfail" != 0 ] || [ -n "$simbad" ]; then
    case "$seat" in S1) eff="guard 1 does not launch (and guard 2 has no valid BASE)" ;; *) eff="guard 2 does not launch; guard 1 can launch alone" ;; esac
    echo "ABORT-REPEAT seat=$seat ($eff)" >> "$OUT"; era "ABORT-REPEAT seat=$seat"; BAD=1
  fi
  rm -rf "$CARGO_TARGET_DIR"
done
echo "REPEAT67-DONE bad=$BAD" >> "$OUT"
era "REPEAT67-DONE bad=$BAD"
[ "$BAD" = 0 ] && exit 0
exit 5
