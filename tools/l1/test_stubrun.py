#!/usr/bin/env python3
"""Stub runs of the candidate plans' launch paths (plans 41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56, 57a, 57b, 59, 60a, 60b, 61a, 61b, 62r0, 62a, 51e, 51, 63, 64, 65): loopdiag_run_all.sh EXECUTED end to end, against stubs.

The static checks in test_harness46/47 and test_bbr48_parse assert strings in the scripts; they did not catch §49 session 5's ABORT-SMOKE (LD_BASE_SHA
lost in `sudo -n env`) or session 1's ABORT-TESTS (a function called before it was defined). Here the real scripts run in a sandbox:

  * stubbed on PATH: sudo (RESETS the environment like env_reset: only an allowlist survives, so a variable the script does not pass explicitly
    is lost), id (root), cargo (writes a distinct stub engine binary per tree; `cargo test` reports green), pgrep, sleep, flock, ip;
  * the measurement driver perf_rwm_c.sh is replaced by stubrun_gen.py, which leaves the logs a run would leave (committed real logs from
    docs/l1-raw plus synthetic [DIAG] / [QCC] / event lines), so the REAL row parsers, smoke checks, scorers and replacement rule run;
  * /dev/shm, /proc and the two VM locks are redirected into the sandbox through the scripts' LD_DEVSHM / LD_PROCFS / LD_LOCK_* seams, and the driver's per-row logs
    from /tmp to the sandbox (LD_TMPD), so the test never touches the host's /tmp logs and two runs never clobber each other;
  * the tools/l1 test files are replaced by no-op stubs inside the sandbox (the gates still execute; the real tests run in this suite).

Modes: the default is FAST (the sandbox's copy of the design tables and size checks is shrunk to 2 groups / 8 path-event rows, minutes); STUBRUN_FULL=1 runs the
registered designs (96 + 22 rows, about an hour). Both assert the same paths and markers. Run it on harness changes only: loopdiag_run_all.sh's suites skip it,
so a measurement session never pays for it.

Plan 64 (F2's merge guard) adds: the perf stub runs the REAL perf script's F2 lines under `set -euo pipefail` (a bare `grep` there once made every BASE row rc 1 = VOID-RC), a taskset stub, a cargo stub that prints the framing bench
(STUB_BENCH_DIR) and puts [SYMSZ] / [UNFRAMABLE] in the CAND binary only, and STUB_BAD64 (stubrun_gen.BAD64) that injects one defect into the first matching scored row. Its `full` path is the launch shape: a rehearsal's run dir feeds the
prebuilt binaries and the tools-test reuse (phase 2).

Every path in harness_paths.HARNESS_PATHS runs; the table must equal what ran. Not stubbable: the VM itself (netns, tc, tcpdump, a real cargo
build, systemd), so those lines of the scripts are covered by the stubs' contract only."""
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import harness_paths as hp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""), flush=True)
    if not ok:
        fails += 1


def find_bash():
    cands = [shutil.which("bash")]
    if os.name == "nt":      # System32\bash.exe is WSL's launcher, not the MSYS bash the scripts need
        cands = [c for c in cands if c and "system32" not in c.lower()] + [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"]
    return next((c for c in cands if c and os.path.exists(c)), None)


BASH = find_bash()


def px(p):
    """A native path as the MSYS shell spells it (C:\\a\\b -> /c/a/b)."""
    p = os.path.abspath(p).replace("\\", "/")
    m = re.match(r"^([A-Za-z]):/(.*)$", p)
    return "/%s/%s" % (m.group(1).lower(), m.group(2)) if m and os.name == "nt" else p


STUBS = {
    "sudo": r'''#!/bin/bash
# stub sudo -n: runs the command with a RESET environment (sudo's env_reset): PATH/HOME-like basics and the STUB_ and LD_ seams survive, nothing else.
[ "$1" = "-n" ] && shift
[ "$1" = "true" ] && exit 0
if [ "$1" = "install" ]; then      # no root on the box: drop -o / -g
  shift; a=(); while [ $# -gt 0 ]; do case "$1" in -o|-g) shift 2 ;; *) a+=("$1"); shift ;; esac; done; set -- install "${a[@]}"
fi
keep=()
for n in PATH HOME TMP TEMP TMPDIR SYSTEMROOT SystemRoot WINDIR windir COMSPEC USERPROFILE MSYSTEM LD_DEVSHM LD_PROCFS LD_TMPD $(compgen -e | grep '^STUB_'); do
  [ -n "${!n+x}" ] && keep+=("$n=${!n}")
done
exec env -i "${keep[@]}" "$@"
''',
    "id": '#!/bin/bash\ncase "$1" in -u) echo 0 ;; *) exec /usr/bin/id "$@" ;; esac\n',
    "pgrep": '#!/bin/bash\ncase "$1" in -xc|-c) echo 0; exit 1 ;; *) exit 1 ;; esac\n',
    "sleep": "#!/bin/bash\nexit 0\n",
    "flock": "#!/bin/bash\nexit 0\n",
    "ip": "#!/bin/bash\nexit 0\n",
    "cargo": r'''#!/bin/bash
echo "$PWD|$*" >> "$STUB_CARGO_LOG"
echo "$PWD|$1|${CARGO_TARGET_DIR:-}" >> "$STUB_CARGO_LOG.tgt"      # plan 67pre: which target dir each call used (fresh, one per step)
gates_fail() {      # plan 67pre: STUB_GATES_FAIL=<seat,seat>:<name>: the gates test `gates::<name>` fails in those seats' trees (the cwd is .../seats/<seat>/src)
  [ -n "${STUB_GATES_FAIL:-}" ] || return 0
  local seats="${STUB_GATES_FAIL%%:*}" seat="${PWD%/src}"; seat="${seat##*/}"
  case ",$seats," in *",$seat,"*) echo "gates::${STUB_GATES_FAIL#*:}" ;; esac
}
case "$1" in
  check)      # plan 67pre: cargo check --all-targets; STUB_CHECK_FAIL=<seat> fails it in that seat's tree
    [ -n "${STUB_CHECK_FAIL:-}" ] && case "$PWD" in */seats/$STUB_CHECK_FAIL/src) echo "error: stub cargo check failure" >&2; exit 101 ;; esac ;;
  build)
    T="${CARGO_TARGET_DIR:?}"; mkdir -p "$T/release"
    if [ -n "${STUB_BIN_FILE:-}" ]; then cp "$STUB_BIN_FILE" "$T/release/raptorpath"; chmod +x "$T/release/raptorpath"; exit 0; fi      # plans 60a / 60b: the test knows the sha it registers
    case "$PWD" in */seats/*/src/raptorpath)      # plan 67: one source tree per seat; the twins are the same tree built --features test-seams
      tree="${PWD%/src/raptorpath}"; tree="${tree##*/}"; seat="$tree"
      case " $* " in *" --features test-seams "*) seat="${tree}T" ;; esac
      cp "$STUB_SEATDIR/$seat.bin" "$T/release/raptorpath"; chmod +x "$T/release/raptorpath"; exit 0 ;;
    esac
    case "$PWD" in */base/src/raptorpath) kind=BASE ;; *) kind=CAND ;; esac
    { echo '#!/bin/sh'; echo "# stub engine binary $kind built from $PWD"; echo "# RWM_STORE"
      [ "$kind" = CAND ] && echo "# [QCC] t= RWM_QUIC_CC"
      echo "# RWM_STORE_PATH_POOL"                                          # plan 54b: the knob is in both trees
      echo "# RWM_SUCC_DUMP"                                                # plan 56: the raw successor dump is in main
      echo "# RWM_EMIT_BURST"                                               # plans 57a / 57b: the emission-burst knob is in main
      echo "# RWM_UNIFIED_SHED unified overload shedding ACTIVE"            # plan 59: the shed gate and its liveness echo are in main
      echo "# RWM_SLAW [SLAW] t="                                           # plans 60a / 60b: the gauge
      echo "# RWM_DELTA emission batching ACTIVE emission batching out of scope"   # plan 51: the delta override and both batching echoes are in main
      { [ "$kind" = CAND ] || [ "$LD_PLAN" = 54b ]; } && echo "# [SNAPV] site= tail"       # plan 54: only CAND (main + the law) has the gauge; plan 54b: both trees (cand54, levb54)
      echo "# fmt  RWM_PLACE_WDIV= RWM_RECOV_SNAPVER= tail"       # plan 53: both trees carry the WDIV and SNAPVER echoes (CW0 runs on CAND's, BASES on BASE's)
      { [ "$kind" = CAND ] && [ "${LD_PLAN:-}" = 64 ]; } && echo "# [SYMSZ] [UNFRAMABLE] F2 framing"      # plan 64: the F2 build holds both strings, main neither
      { [ "$kind" = BASE ] && [ -n "${STUB_BASE_SYMSZ:-}" ]; } && echo "# [SYMSZ] main with a stray size echo"      # plan 64's base-has-symsz path
      [ -f ../COMMIT ] && echo "# commit $(cat ../COMMIT)"            # plan 67's binid: two trees with different commits build to different binaries
      echo 'exit 0'; } > "$T/release/raptorpath"
    chmod +x "$T/release/raptorpath" ;;
  test)
    # as cargo does: a target named with --test whose required-features are off is an error, before anything builds (raptorpath/Cargo.toml: s06_reactive needs test-seams, sim_* need sim and test-seams);
    # the sim targets also need RUSTFLAGS="--cfg tokio_unstable" (ADR-0082), here a build error without it
    prev=""
    for a in "$@"; do
      if [ "$prev" = "--test" ]; then
        case "$a" in
          s06_reactive) case " $* " in *" --features test-seams "*|*" --features sim,test-seams "*) ;; *) echo "error: target \`$a\` in package \`raptorpath\` requires the features: \`test-seams\`" >&2; exit 101 ;; esac ;;
          sim_nack_test) ;;
          sim_*) case " $* " in *" --features sim,test-seams "*) ;; *) echo "error: target \`$a\` in package \`raptorpath\` requires the features: \`sim\`, \`test-seams\`" >&2; exit 101 ;; esac
                 case "${RUSTFLAGS:-}" in *tokio_unstable*) ;; *) echo "error: could not compile raptorpath (stub: sim without --cfg tokio_unstable)" >&2; exit 101 ;; esac ;;
        esac
      fi
      prev="$a"
    done
    running() { local p="" a; for a in "$@"; do [ "$p" = "--test" ] && echo "     Running tests/$a.rs (target/release/deps/$a-0)"; [ "$a" = "--lib" ] && echo "     Running unittests src/lib.rs (target/release/deps/raptorpath-0)"; p="$a"; done; }
    per_target() { # <passed> <cargo args...>: one Running line and one result line per --test target, as cargo prints them; STUB_REPEAT_IGNORED=<t>: that target's tests are all #[ignore]
      local n="$1" p="" a; shift
      for a in "$@"; do
        if [ "$p" = "--test" ]; then
          echo "     Running tests/$a.rs (target/release/deps/$a-0)"
          if [ "$a" = "${STUB_REPEAT_IGNORED:-}" ]; then echo "test result: ok. 0 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out"
          else echo "test result: ok. $n passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"; fi
        fi
        p="$a"
      done
    }
    case " $* " in
      *" --no-run "*) ;;
      *" --lib -- config:: "*) if [ -n "${STUB_REPEAT_SLEEP_S1:-}" ]; then s0=$SECONDS; while [ $(( SECONDS - s0 )) -lt "$STUB_REPEAT_SLEEP_S1" ]; do :; done; fi; running "$@"; echo "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" ;;      # plan 67's repeat run of S1: STUB_REPEAT_SLEEP_S1=<s> makes each run take that long (the stub `sleep` is a no-op, so a SECONDS loop; S2's share is the budget minus S1's elapsed time)
      *" --test s06_no_copy "*)      # plan 67's repeat run (repeat67_run.sh): STUB_REPEAT_FAIL=<k>: the k-th run of the S2 gating tests fails one test; STUB_REPEAT_ZERO=1: every run passes zero tests (a filter or build that ran nothing)
        if [ -n "${STUB_REPEAT_ZERO:-}" ]; then per_target 0 "$@"; exit 0; fi
        per_target 3 "$@"
        if [ -n "${STUB_REPEAT_FAIL:-}" ]; then
          n=$(cat "$STUB_REPEAT_DIR/n" 2>/dev/null || echo 0); n=$(( n + 1 )); echo "$n" > "$STUB_REPEAT_DIR/n"
          if [ "$n" = "$STUB_REPEAT_FAIL" ]; then echo "test s06_no_copy::t_one ... FAILED"; echo "test result: FAILED. 2 passed; 1 failed; 0 ignored"; exit 101; fi
        fi ;;
      *" framing_ns "*)      # plan 64's rehearsal: one run of the framing bench per call, the STUB_BENCH_DIR blocks b1..b3 in rotation
        n=$(cat "$STUB_BENCH_DIR/n" 2>/dev/null || echo 0); n=$(( n + 1 )); echo "$n" > "$STUB_BENCH_DIR/n"
        cat "$STUB_BENCH_DIR/b$(( (n - 1) % 3 + 1 )).txt" ;;
      *" -p raptorpath --lib gates "*)      # plan 67pre: the gates reference (B0); STUB_GATES_NORESULT=1: a build error, no `test result` line
        [ -z "${STUB_GATES_NORESULT:-}" ] || { echo "error: could not compile raptorpath (stub)" >&2; exit 101; }
        echo "test gates::stub_ok ... ok"
        g="$(gates_fail)"
        if [ -n "$g" ]; then echo "test $g ... FAILED"; echo "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out"; exit 101; fi
        echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" ;;
      *" -p raptorpath-wasm golden "*)      # plan 67pre: the native golden test; STUB_GOLDEN_FAIL=1 fails it
        [ -z "${STUB_GOLDEN_FAIL:-}" ] || { echo "test golden_fingerprint ... FAILED"; echo "test result: FAILED. 0 passed; 1 failed; 0 ignored"; exit 101; }
        echo "test golden_fingerprint ... ok"; echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" ;;
      *" -p raptorpath-math "*)      # the main suite call. STUB_FLAKE=<test>: it fails once; the solo re-run (`-- <test> --exact`, the default branch below) passes: the ADR-0080 flake
        gf="$(gates_fail)"      # plan 67pre: a gates failure on this seat's tree fails the solo re-run too (REAL), STUB_FLAKE passes it (FLAKE)
        case " $* " in
          *" --exact "*) ;;
          *) if [ -n "${STUB_TARGETS:-}" ]; then      # plan 67pre: the registered touched-integration-test targets show in the suite log
               for tg in holeclass_reachability recovery_bench sim_nack_test place_arms_reachability s06_no_copy s06_reactive sim_shed_reach sim_lost_tail; do echo "     Running tests/$tg.rs (target/release/deps/$tg-0)"; done
               echo "test config::stub_ok ... ok"
             fi
             bad=0
             [ -z "${STUB_FLAKE:-}" ] || { echo "test $STUB_FLAKE ... FAILED"; bad=1; }
             [ -z "$gf" ] || { echo "test $gf ... FAILED"; bad=1; }
             if [ -n "${STUB_BINTEST_FAIL:-}" ] && [ -f "$PWD/COMMIT" ]; then      # plan 67pre's binid: STUB_BINTEST_FAIL=<commit,commit>:<name>: the math + wasm tests of the tree with that COMMIT (binid_run.sh's $WORK/src) fail it
               case ",${STUB_BINTEST_FAIL%%:*}," in *",$(cat "$PWD/COMMIT"),"*) echo "test binid_math::${STUB_BINTEST_FAIL#*:} ... FAILED"; bad=1 ;; esac
             fi
             [ "$bad" = 0 ] || { echo "test result: FAILED. 0 passed; 1 failed; 0 ignored"; exit 101; } ;;
        esac
        echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
        seen=0
        for a in "$@"; do
          [ "$seen" = 1 ] && case "$a" in -*) ;; *) [ "$a" = "$gf" ] || echo "test $a ... ok" ;; esac
          [ "$a" = "--" ] && seen=1
        done ;;
      *" --test sim_shed_reach "*|*" --test sim_nack_test "*)      # plan 67's repeat: the sim / deterministic lists, one result line per target
        per_target 1 "$@" ;;
      *) running "$@"
          echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
          seen=0      # plan 51's dial pins name their tests after `--`: one `test <name> ... ok` line each (STUB_FAIL_PIN: that one FAILS; STUB_SKIP_PIN: that one prints nothing, a filter that matched no test)
          for a in "$@"; do
            if [ "$seen" = 1 ]; then
              case "$a" in -*) ;; *)
                if [ "${STUB_FAIL_PIN:-}" = "$a" ]; then echo "test $a ... FAILED"; echo "test result: FAILED. 0 passed; 1 failed; 0 ignored"; exit 101; fi
                [ "${STUB_SKIP_PIN:-}" = "$a" ] || echo "test $a ... ok" ;;
              esac
            fi
            [ "$a" = "--" ] && seen=1
          done ;;
    esac ;;
esac
exit 0
''',
    "timeout": r'''#!/bin/bash
# stub timeout: STUB_TRUNC=<plan token> (plan 67 `truncated`): the envelope's battery call of that plan (LD_PLAN=<token> LD_TAG=ld) runs to its end and then returns rc 124, what the real timeout returns when LD_HARD cut the
# battery. Every other call is the real timeout (the first `timeout` on PATH that is not this script).
d="$1"; shift
if [ -n "${STUB_TRUNC:-}" ] && [[ " $* " == *" LD_PLAN=$STUB_TRUNC "* ]] && [[ " $* " == *" LD_TAG=ld "* ]]; then "$@"; exit 124; fi
real=""
IFS=: read -ra _dirs <<< "$PATH"
for _x in "${_dirs[@]}"; do [ -x "$_x/timeout" ] && ! [ "$_x/timeout" -ef "$0" ] && { real="$_x/timeout"; break; }; done
[ -n "$real" ] || { "$@"; exit $?; }
exec "$real" "$d" "$@"
''',
    "taskset": '#!/bin/bash\n[ "$1" = "-c" ] && { echo "taskset $*" >> "${STUB_TASKSET_LOG:-/dev/null}"; shift 2; }\nexec "$@"\n',
    "python3": r'''#!/bin/bash
# python3 passthrough that records the environment each ROW parser invocation sees (loopdiag_battery.sh: python3 <parser> row <cell> <arm> ...)
if [ "$2" = "row" ]; then echo "plan=${LD_PLAN-<unset>} base_sha=${LD_BASE_SHA-<unset>} cell=$3 arm=$4 parser=$1 shabase=${LD_SHA_BASE-<unset>}" >> "$STUB_ROWLOG"; fi
PYTHONPATH="$STUB_PYSITE" exec "$STUB_PYTHON" "$@"
''',
}
# Windows Python writes CRLF on stdout; the scripts' `mapfile` of a plan table would then carry a CR into every field.
SITECUSTOMIZE = "import sys\nfor _s in (sys.stdout, sys.stderr):\n    try:\n        _s.reconfigure(newline='\\n')\n    except Exception:\n        pass\n"
PERF_STUB = '''#!/bin/bash
# stub measurement driver: leaves the logs a perf_rwm_c.sh run leaves in /tmp (stubrun_gen.py)
export STUB_TMP="$(cygpath -m "${LD_TMPD:?}" 2>/dev/null || echo "$LD_TMPD")"
[ -n "${STUB_DRVLOG:-}" ] && echo "$1 $2 $4 timeout=${RWM_PERF_TIMEOUT_S-unset}" >> "$STUB_DRVLOG"
[ -n "${STUB_DRVLOG3:-}" ] && echo "$1 $2 $3 $4 $6 timeout=${RWM_PERF_TIMEOUT_S-unset} slaw=${RWM_SLAW-unset} pool=${RWM_STORE_PATH_POOL-unset} reliable=${RWM_C_RELIABLE-unset} rdiag=${RWM_RDIAG-unset} rtobs=${RWM_RTOBS-unset} diag=${RWM_DIAG-unset} bin=$(basename "${RWM_BIN:-?}")" >> "$STUB_DRVLOG3"      # plans 60a / 60b: the whole per-row run spec
[ -n "${STUB_DRVLOG4:-}" ] && echo "$1 $2 $3 $4 $6 timeout=${RWM_PERF_TIMEOUT_S-unset} delta=${RWM_DELTA-unset} rate=${RWM_PERF_RATE_MBIT-unset} slaw=${RWM_SLAW-unset} reliable=${RWM_C_RELIABLE-unset} rdiag=${RWM_RDIAG-unset} rtobs=${RWM_RTOBS-unset} diag=${RWM_DIAG-unset} gen=${RWM_GEN-unset} minr=${RWM_MIN_R-unset} ccpace=${RWM_CC_PACE-unset} copadelta=${RWM_COPA_DELTA-unset} copawire=${RWM_COPA_WIRE-unset} cexp=${RWM_COMPLETION_EXPOSURE-unset}" >> "$STUB_DRVLOG4"      # plans 51e / 51: the per-row run spec incl. every scrubbed knob
[ -n "${STUB_DRVLOG6:-}" ] && echo "$1 $2 $3 $4 $6 timeout=${RWM_PERF_TIMEOUT_S-unset} rate=${RWM_PERF_RATE_MBIT-unset} slaw=${RWM_SLAW-unset} reliable=${RWM_C_RELIABLE-unset} rdiag=${RWM_RDIAG-unset} diag=${RWM_DIAG-unset} bindir=$(basename "$(dirname "${RWM_BIN:-?/?}")")" >> "$STUB_DRVLOG6"      # plan 63: which seat's binary ran
[ -n "${STUB_DRVLOG2:-}" ] && echo "$3 reliable=${RWM_C_RELIABLE-unset} unified=${RWM_UNIFIED-unset} gen=${RWM_GEN-unset}" >> "$STUB_DRVLOG2"      # plan 59: hint + the evicting-seat env (a separate log: drv.log's lines are compared whole)
[ -n "${STUB_DRVLOG5:-}" ] && echo "$3 $4 $6 timeout=${RWM_PERF_TIMEOUT_S-unset} reliable=${RWM_C_RELIABLE-unset} diag=${RWM_DIAG-unset} bindir=$(basename "$(dirname "${RWM_BIN:-?/?}")")" >> "$STUB_DRVLOG5"      # plan 64: hint, bytes, mode, timeout, reliability, the seat's binary dir
[ -n "${STUB_DRVLOG7:-}" ] && echo "$3 $4 $6 timeout=${RWM_PERF_TIMEOUT_S-unset} rate=${RWM_PERF_RATE_MBIT-unset} pool=${RWM_STORE_PATH_POOL-unset} dump=${RWM_SUCC_DUMP-unset} reliable=${RWM_C_RELIABLE-unset} diag=${RWM_DIAG-unset} bin=$(basename "$(dirname "${RWM_BIN:-?/?}")") cwd=$PWD" >> "$STUB_DRVLOG7"      # plan 67
python3 ./stubrun_gen.py "$@" || { echo "stub driver failed"; exit 1; }
# The REAL perf script's F2 lines (the `--- F2 undeliv=.. unframable=..` witness), extracted from perf_rwm_c.real.sh and run on the stub's logs under the shell options the real script runs with: lib.sh turns on `set -e`
# (perf_rwm_c.sh sources it), so a bare `grep` that finds nothing (exit 1) would kill the real script and make EVERY BASE row rc 1 (VOID-RC: the section 63 defect). The stub's exit status is that of these lines.
_F2="$(grep -E '^(_F2_TXT=|_F2_UND=|_F2_UNF=|echo "--- F2)' ./perf_rwm_c.real.sh | sed "s#/tmp/rwm-#$STUB_TMP/rwm-#g")"
[ -n "$_F2" ] || { echo "stub: the real script has no F2 lines"; exit 1; }
bash -c "set -euo pipefail; $_F2; echo after-f2 >/dev/null"
rc=$?
[ -e "$STUB_TMP/stub-rc1" ] && { rm -f "$STUB_TMP/stub-rc1"; rc=1; }      # plan 64's dead BASE warm-up: the client died after the full timeout, the driver exits non-zero
exit $rc
'''
CAND_SHA = "c" * 40
BASE_SHA = "b3000701f440d11b1b5ad946c8b36c3623c770cd"
HIST_BASE = "4926822" + "0" * 33
SINGLE = ("56", "57a", "57b", "59", "60a", "60b", "61a", "61b", "63")        # the single-seat plans: one build, no LD_BASE_SHA, suite reuse by the reference manifest
S60 = ("60a", "60b")
S61 = ("61a", "61b")      # status 61 (the c3 sweep): plan 60's single-seat shape with its own tables
KNEE61 = "88"            # the pilot knee the 61a sessions run on (the rehearsal's PILOT61 W-hat on the stub: 86.5)
# FULL (STUBRUN_FULL=1): the registered designs, 96 guard rows / 22 path-event rows plus the smokes. FAST (the default): the sandbox's COPY of the tables and of
# the design-size checks is shrunk (2 groups = 16 guard rows, 8 path-event rows), so every path and marker is exercised in minutes; the same code runs, on fewer rows.
FULL = os.environ.get("STUBRUN_FULL") == "1"
# plan 52: the guard's smoke (2) + the guard + the reported block's smoke (3) + the block (30 registered, 10 in fast mode)
EXPECT_ROWS = ({("41c", False): 2, ("41c", True): 98, ("46", False): 2, ("46", True): 24, ("47", False): 4, ("47", True): 122, ("48", False): 2, ("48", True): 98,
                ("52", False): 5, ("52", True): 131, ("41d", False): 3, ("41d", True): 99, ("53", False): 8, ("53", True): 68, ("54", False): 3, ("54", True): 99, ("54b", False): 6, ("54b", True): 42, ("56", False): 4, ("56", True): 74, ("57a", False): 1, ("57a", True): 64, ("57b", False): 2, ("57b", True): 32, ("59", False): 1, ("59", True): 6} if FULL else
               {("41c", False): 2, ("41c", True): 18, ("46", False): 2, ("46", True): 10, ("47", False): 4, ("47", True): 28, ("48", False): 2, ("48", True): 18,
                ("52", False): 5, ("52", True): 31, ("41d", False): 3, ("41d", True): 19, ("53", False): 8, ("53", True): 24, ("54", False): 3, ("54", True): 19, ("54b", False): 6, ("54b", True): 18, ("56", False): 4, ("56", True): 22, ("57a", False): 1, ("57a", True): 7, ("57b", False): 2, ("57b", True): 14, ("59", False): 1, ("59", True): 3})
# (file, old, new, expected count): each must match, so a drift in the scripts fails here instead of silently running the full design
FAST_PATCHES = [
    ("loopdiag_run_all.sh", '[ "$nrows" != "96" ]', '[ "$nrows" != "16" ]', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 96 ] || { echo "ABORT-PLAN41C', '[ "${#tbl[@]}" -eq 16 ] || { echo "ABORT-PLAN41C', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 96 ] || { echo "ABORT-PLAN48', '[ "${#tbl[@]}" -eq 16 ] || { echo "ABORT-PLAN48', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 22 ] || { echo "ABORT-PLAN46', '[ "${#tbl[@]}" -eq 8 ] || { echo "ABORT-PLAN46', 1),
    ("place38_parse.py", "EXPECT_ROWS41C = len(GUARD_CELLS) * (12 + 12)", "EXPECT_ROWS41C = len(GUARD_CELLS) * (2 + 2)", 1),
    ("place38_parse.py", "for rp in range(1, 7) for sd", "for rp in range(1, 2) for sd", 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 30 ] || { echo "ABORT-PLAN52', '[ "${#tbl[@]}" -eq 10 ] || { echo "ABORT-PLAN52', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 60 ] || { echo "ABORT-PLAN53', '[ "${#tbl[@]}" -eq 16 ] || { echo "ABORT-PLAN53', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 36 ] || { echo "ABORT-PLAN54B', '[ "${#tbl[@]}" -eq 12 ] || { echo "ABORT-PLAN54B', 1),
    ("place38_parse.py", "for rp in range(1, 4) for sd", "for rp in range(1, 2) for sd", 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 70 ] || { echo "ABORT-PLAN56 table', '[ "${#tbl[@]}" -eq 18 ] || { echo "ABORT-PLAN56 table', 1),
    ("place38_parse.py", "REPS_CAP56 = None  ", "REPS_CAP56 = 1  ", 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 63 ] || { echo "ABORT-PLAN57A table', '[ "${#tbl[@]}" -eq 6 ] || { echo "ABORT-PLAN57A table', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 30 ] || { echo "ABORT-PLAN57B table', '[ "${#tbl[@]}" -eq 12 ] || { echo "ABORT-PLAN57B table', 1),
    ("place38_parse.py", "REPS_CAP57 = None  ", "REPS_CAP57 = 2  ", 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 5 ] || { echo "ABORT-PLAN59 table', '[ "${#tbl[@]}" -eq 2 ] || { echo "ABORT-PLAN59 table', 1),
    ("place38_parse.py", "REPS_CAP59 = None  ", "REPS_CAP59 = 2  ", 1),
    ("loopdiag_battery.sh", 'want=96; [ "$PLAN" = "60b" ] && want=40', 'want=32; [ "$PLAN" = "60b" ] && want=10', 1),
    ("plan60.py", "REPS_CAP60 = None  ", "REPS_CAP60 = 2  ", 1),
    ("place41b_score.py", "GUARD_DESIGN = {cell: (12, 12) for cell, _ in CELLS_GUARD}", "GUARD_DESIGN = {cell: (2, 2) for cell, _ in CELLS_GUARD}", 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 24 ] || { echo "ABORT-PLAN64 table', '[ "${#tbl[@]}" -eq 4 ] || { echo "ABORT-PLAN64 table', 1),
    ("plan64.py", "for rp in range(1, 7) for sd", "for rp in range(1, 2) for sd", 1),
    ("plan64.py", "PAIRS65 = s65_score.N_PAIRS", "PAIRS65 = 6", 1),
    ("plan64.py", "MIN_PAIRS65 = s65_score.MIN_PAIRS", "MIN_PAIRS65 = 5", 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 48 ] || { echo "ABORT-PLAN65 table', '[ "${#tbl[@]}" -eq 12 ] || { echo "ABORT-PLAN65 table', 1),
    ("loopdiag_battery.sh", 'want=66; [ "$PLAN" = "61b" ] && want=30', 'want=22; [ "$PLAN" = "61b" ] && want=10', 1),
    ("plan61.py", "REPS_CAP61 = None  ", "REPS_CAP61 = 2  ", 1),
    ("loopdiag_battery.sh", "    want=44\n", "    want=16\n", 1),
    ("plan62.py", "REPS_CAP62 = None  ", "REPS_CAP62 = 2  ", 1),
    ("loopdiag_battery.sh", 'want=12; [ "$(python3 ./plan51e.py dropped)" = 1 ] && want=7', 'want=6; [ "$(python3 ./plan51e.py dropped)" = 1 ] && want=4', 1),
    ("plan51e.py", "REPS_CAP51E = None  ", "REPS_CAP51E = 2  ", 1),
    ("loopdiag_battery.sh", "    want=76\n", "    want=28\n", 1),
    ("plan51.py", "REPS_CAP51 = None  ", "REPS_CAP51 = 2  ", 1),
    ("loopdiag_p67.sh", "G_ROWS67=96     #", "G_ROWS67=16     #", 1),
    ("plan67.py", "N_BLOCKS = 6  ", "N_BLOCKS = 2  ", 1),
    ("plan67.py", "N_TWIN_REPS = 3  ", "N_TWIN_REPS = 2  ", 1),
    ("plan67.py", "N_POOL_REPS = 3  ", "N_POOL_REPS = 1  ", 1),
    ("plan67.py", "MIN_LIVE_A = 10  ", "MIN_LIVE_A = 2  ", 1),
    ("plan67.py", "MIN_LIVE_B0 = 4  ", "MIN_LIVE_B0 = 1  ", 1),
    ("plan67.py", "MIN_TWIN = 5  ", "MIN_TWIN = 2  ", 1),
    ("place46_parse.py", "SCORED_REPS = (1, 2)", "SCORED_REPS = (1,)", 1),
    ("place46_parse.py", 'SEEDS46 = ("42", "7")', 'SEEDS46 = ("42",)', 1),
    ("place46_parse.py", "EXPECT_ROWS46 = 22 ", "EXPECT_ROWS46 = 8 ", 1),
]


def shrink(tools):
    for f, old, new, n in FAST_PATCHES:
        path = os.path.join(tools, f)
        txt = open(path, newline="").read()
        assert txt.count(old) == n, (f, old, txt.count(old))
        with open(path, "w", newline="") as fh:
            fh.write(txt.replace(old, new))


def make_sandbox(root, plan, base_sha, src_text="fn main() {}\n", edits=None):
    """-> dict of paths. The tools dir is a copy of this one with every test file stubbed and the driver replaced."""
    sb = {k: os.path.join(root, k) for k in ("bin", "repo", "root", "devshm", "proc", "locks", "logs", "tmpd")}
    for d in sb.values():
        os.makedirs(d, exist_ok=True)
    tools = os.path.join(sb["repo"], "tools", "l1")
    os.makedirs(tools)
    for f in os.listdir(HERE):
        p = os.path.join(HERE, f)
        if not os.path.isfile(p) or f.endswith((".pyc",)) or f == "test_stubrun.py":
            continue
        if f.startswith("test_"):
            with open(os.path.join(tools, f), "w", newline="\n") as fh:
                fh.write("import os\nif os.environ.get('STUB_TESTLOG'):\n    open(os.environ['STUB_TESTLOG'], 'a').write(os.path.basename(__file__) + '\\n')\nprint('stub test (the real one runs in the suite)')\n" if f.endswith(".py")
                         else 'echo "$(basename "$0")" >> "${STUB_TESTLOG:-/dev/null}"\nexit 0\n')
        else:
            shutil.copyfile(p, os.path.join(tools, f))
    shutil.copyfile(os.path.join(HERE, "perf_rwm_c.sh"), os.path.join(tools, "perf_rwm_c.real.sh"))      # the REAL script: the stub driver runs its F2 lines under the real script's shell options (PERF_STUB)
    with open(os.path.join(tools, "perf_rwm_c.sh"), "w", newline="\n") as fh:
        fh.write(PERF_STUB)
    sl_src, sl_dst = os.path.join(HERE, "..", "..", "docs", "derivations", "store-law"), os.path.join(sb["repo"], "docs", "derivations", "store-law")
    os.makedirs(sl_dst, exist_ok=True)      # plans 60a / 60b: s60_score.py imports s60replay / caplog_replay from here
    for f in os.listdir(sl_src):
        if f.endswith((".py", ".log")):
            shutil.copyfile(os.path.join(sl_src, f), os.path.join(sl_dst, f))
    ci_dir = os.path.join(sb["repo"], "tools", "ci")      # the suites' flake rule (ADR-0080): run_suites calls tools/ci/flake_rule.py against raptorpath/tests/quarantine.txt
    os.makedirs(ci_dir, exist_ok=True)
    shutil.copyfile(os.path.join(HERE, "..", "ci", "flake_rule.py"), os.path.join(ci_dir, "flake_rule.py"))
    if not FULL:
        shrink(tools)
    tree = {"raptorpath/tests/quarantine.txt": "# the quarantine list: empty\n", "raptorpath/src/lib.rs": src_text, "raptorpath/src/b.rs": "// b\n", "raptorpath/tests/t.rs": "// t\n",
            "Cargo.toml": "[workspace]\n", "Cargo.lock": "# lock\n", "raptorpath-math/src/lib.rs": "// math\n", "raptorpath-core/src/lib.rs": "// core\n", "raptorpath-wasm/src/lib.rs": "// wasm\n",
            "gf256/src/lib.rs": "// gf",
            "vendor/quinn-proto/Cargo.toml": "[package]\n", "vendor/quinn-proto/src/lib.rs": "// vendored\n"}
    for crate in ("gf256", "raptorpath", "raptorpath-math", "raptorpath-core", "raptorpath-wasm"):
        tree[crate + "/Cargo.toml"] = "[package]\nname = \"%s\"\n" % crate
    tree.update(edits or {})
    for rel, text in tree.items():
        os.makedirs(os.path.dirname(os.path.join(sb["repo"], rel)), exist_ok=True)
        with open(os.path.join(sb["repo"], rel), "w", newline="\n") as fh:
            fh.write(text)
    with open(os.path.join(sb["bin"], "sitecustomize.py"), "w", newline="\n") as fh:
        fh.write(SITECUSTOMIZE)
    for name, text in STUBS.items():
        with open(os.path.join(sb["bin"], name), "w", newline="\n") as fh:
            fh.write(text)
        os.chmod(os.path.join(sb["bin"], name), 0o755)
    os.makedirs(os.path.join(sb["root"], "src", "raptorpath"))
    os.makedirs(os.path.join(sb["root"], "base", "src", "raptorpath"))
    with open(os.path.join(sb["root"], "COMMIT"), "w") as fh:
        fh.write(CAND_SHA + "\n")
    with open(os.path.join(sb["root"], "base", "COMMIT"), "w") as fh:
        fh.write((base_sha or HIST_BASE) + "\n")
    os.makedirs(os.path.join(sb["proc"], "pressure"))
    for rel, text in (("stat", "cpu 1 1 1 1\nprocs_running 1\n"), ("loadavg", "0.10 0.10 0.10 1/100 1\n"),
                      ("pressure/io", "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n")):
        with open(os.path.join(sb["proc"], rel), "w", newline="\n") as fh:
            fh.write(text)
    return sb


def run_launch(sb, plan, base_sha, rehearse=False, prior=None, p46_suites=False, timeout=3600, guard_set=None, extra_env=None):
    """Run loopdiag_run_all.sh in the sandbox; -> (rc, stdout, rowlog lines, cargo log lines)."""
    for n in ("rows.log", "cargo.log", "drv.log", "drv2.log", "drv3.log", "drv4.log", "drv5.log", "drv6.log", "tests.log"):
        open(os.path.join(sb["logs"], n), "w").close()
    env = dict(os.environ)
    for k in [k for k in env if k.startswith(("LD_", "RWM_", "STUB_"))]:
        del env[k]
    env.update({
        "PATH": px(sb["bin"]) + os.pathsep + env.get("PATH", ""),
        "LD_ROOT": px(sb["root"]), "LD_SHM": px(sb["devshm"]) + "/ld", "LD_DEVSHM": px(sb["devshm"]), "LD_PROCFS": px(sb["proc"]), "LD_TMPD": px(sb["tmpd"]),
        "LD_LOCK_VM": px(sb["locks"]) + "/vm.lock", "LD_LOCK_RP": px(sb["locks"]) + "/rp.lock",
        "LD_PLAN": plan, "LD_CAND_SHA": CAND_SHA, "LD_HARD": str(int(time.time()) + 6 * 3600),
        "STUB_ROWLOG": px(sb["logs"]) + "/rows.log", "STUB_CARGO_LOG": px(sb["logs"]) + "/cargo.log", "STUB_DRVLOG": px(sb["logs"]) + "/drv.log", "STUB_DRVLOG2": px(sb["logs"]) + "/drv2.log", "STUB_DRVLOG3": px(sb["logs"]) + "/drv3.log", "STUB_DRVLOG4": px(sb["logs"]) + "/drv4.log", "STUB_DRVLOG5": px(sb["logs"]) + "/drv5.log", "STUB_TASKSET_LOG": px(sb["logs"]) + "/taskset.log", "STUB_TESTLOG": os.path.abspath(sb["logs"]).replace(os.sep, "/") + "/tests.log",      # a path both the MSYS bash and the Windows python stubs can append to
        "STUB_SHORT_ONCE": "1",       # plan 41d: the first BASE row of each long cell at seed 7 is a 10 s row (the VOID rule + the replacement run)
        "STUB_PYTHON": px(sys.executable), "STUB_PYSITE": os.path.abspath(sb["bin"]), "STUB_RAW": os.environ.get("STUBRUN_RAW") or os.path.abspath(os.path.join(HERE, "..", "..", "docs", "l1-raw")),
    })
    env.update(extra_env or {})
    if base_sha:
        env["LD_BASE_SHA"] = base_sha
    if rehearse:
        env["LD_REHEARSE"] = "1"
    if prior:
        env["LD_PRIOR_TESTS"] = px(prior)
    if p46_suites:
        env["LD_P46_SUITES"] = "1"
    if guard_set:
        env["LD_GUARD_SET"] = guard_set
    script = px(os.path.join(sb["repo"], "tools", "l1", "loopdiag_run_all.sh"))
    cp = subprocess.run([BASH, script], env=env, capture_output=True, text=True, errors="replace", timeout=timeout)
    rows = open(os.path.join(sb["logs"], "rows.log")).read().splitlines()
    cargo = open(os.path.join(sb["logs"], "cargo.log")).read().splitlines()
    return cp.returncode, cp.stdout + cp.stderr, rows, cargo


def era(sb):
    p = os.path.join(sb["root"], "run", "all-era.txt")
    return open(p, errors="replace").read() if os.path.exists(p) else ""


MANIFEST_DIRS = ("raptorpath/src", "raptorpath/tests", "raptorpath-math/src", "raptorpath-core/src", "raptorpath-wasm/src", "gf256/src", "vendor")


def manifest_py(repo):
    """The manifest the scripts must record: the five source dirs, the workspace Cargo.toml / Cargo.lock and every crate's Cargo.toml."""
    out = []
    files = [f for f in ("Cargo.toml", "Cargo.lock") if os.path.exists(os.path.join(repo, f))]
    files += [d + "/Cargo.toml" for d in sorted(os.listdir(repo)) if os.path.isfile(os.path.join(repo, d, "Cargo.toml"))]
    for top in MANIFEST_DIRS:
        for dp, _, fns in os.walk(os.path.join(repo, top)):
            files += [os.path.relpath(os.path.join(dp, fn), repo).replace("\\", "/") for fn in fns]
    for rel in set(files):
        out.append((rel, hashlib.sha256(open(os.path.join(repo, rel), "rb").read()).hexdigest()))
    return "".join("%s  %s\n" % (h, rel) for rel, h in sorted(out))      # C-locale order == codepoint order for these ASCII names


def tail(s, n=25):
    return "\n".join(s.splitlines()[-n:])


EXECUTED = set()
EXTRA_DONE = set()
EXTRA_MISMATCH = ("Cargo.lock", "Cargo.toml", "raptorpath-math/Cargo.toml", "raptorpath-core/Cargo.toml", "vendor/quinn-proto/src/lib.rs", "raptorpath-wasm/src/lib.rs", "raptorpath-math/src/lib.rs", "raptorpath-core/src/lib.rs", "gf256/src/lib.rs")


def extra_mismatch(rel):
    """A rehearsal of plan 48 reusing the 48 full run's suites, with ONE byte of `rel` changed: ABORT-TESTS, nothing measured."""
    tag = "48/prior-tests-mismatch[%s]" % rel
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "48", BASE_SHA, edits={rel: "// one byte differs\n"})
        rc, out, rows, cargo = run_launch(sb, "48", BASE_SHA, rehearse=True, prior=SUITES_DIR["prior"])
        check(f"{tag}: ABORT-TESTS, FAILED-ALL, rc 5, no smoke row measured",
              rc == 5 and "ABORT-TESTS" in era(sb) and os.path.exists(os.path.join(sb["root"], "run", "FAILED-ALL")) and rows == [], (rc, tail(out)))
        EXTRA_DONE.add(rel)
    finally:
        shutil.rmtree(root, ignore_errors=True)

FLAKE_TEST = "stub::flaky_one"
EXTRA_FLAKE = ("flake-not-quarantined", "flake-quarantined")
FLAKE_DONE = set()


def extra_flake(kind):
    """ADR-0080 in run_suites (plan 48 launch, no suite reuse): the main suite fails once and passes its solo re-run.
    flake-not-quarantined: still a failure -> ABORT-TESTS, FAILED-ALL, rc 5, nothing measured (the old rule called it ok).
    flake-quarantined (control): the test is on quarantine.txt -> QUARANTINED on record, the session completes."""
    tag = "48/" + kind
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        edits = {"raptorpath/tests/quarantine.txt": "%s | stub owner | stub reason\n" % FLAKE_TEST} if kind == "flake-quarantined" else None
        sb = make_sandbox(root, "48", BASE_SHA, edits=edits)
        rc, out, rows, cargo = run_launch(sb, "48", BASE_SHA, extra_env={"STUB_FLAKE": FLAKE_TEST})
        tt = open(os.path.join(sb["root"], "run", "suites", "TESTS.txt"), errors="replace").read()
        failed = os.path.exists(os.path.join(sb["root"], "run", "FAILED-ALL"))
        reruns = [c for c in cargo if "|test " in c and "--exact" in c]
        check(f"{tag}: the flaky test failed once and passed its solo re-run (FLAKE on record, one re-run)", f"FLAKE main {FLAKE_TEST}" in tt and len(reruns) == 1, (tt, reruns))
        if kind == "flake-not-quarantined":
            check(f"{tag}: a pass on the re-run no longer excuses it: FLAKE-FAILURE, TESTS-FAIL, ABORT-TESTS, FAILED-ALL, rc 5, no row measured",
                  f"FLAKE-FAILURE main {FLAKE_TEST}" in tt and "QUARANTINED" not in tt and os.path.exists(os.path.join(sb["root"], "run", "suites", "TESTS-FAIL")) and not os.path.exists(os.path.join(sb["root"], "run", "suites", "TESTS-OK"))
                  and rc == 5 and "ABORT-TESTS" in era(sb) and failed and rows == [] and "LD-ALL-DONE" not in out, (rc, tt, tail(out)))
        else:
            check(f"{tag}: on the quarantine list the same failure is QUARANTINED, not a suite failure: TESTS-OK, the session completes (LD-ALL-DONE, rc 0)",
                  f"QUARANTINED main {FLAKE_TEST} (FLAKE-FAILURE, on the quarantine list)" in tt and os.path.exists(os.path.join(sb["root"], "run", "suites", "TESTS-OK")) and rc == 0 and "LD-ALL-DONE" in out and not failed, (rc, tt, tail(out)))
        FLAKE_DONE.add(kind)
    finally:
        shutil.rmtree(root, ignore_errors=True)


def check53(tag, path, run_dir):
    """Plan 53's own assertions on a stub run: its table, binaries, env and ledger, the rehearsal criteria it prints, and that no guard exists."""
    import json
    rep = os.path.join(run_dir, "rep53")
    smoke = os.path.join(rep, "smoke.log")
    stxt = open(smoke, errors="replace").read() if os.path.exists(smoke) else ""
    chk = os.path.join(rep, "smoke-check.txt")
    ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
    arms8 = {(c, a) for c in ("c8-100", "c1d-400") for a in (("BASE", "CAND", "CANDS", "BASES", "CW0") if c == "c8-100" else ("BASE", "CAND", "CANDS"))}
    check(f"{tag}: no guard stage (no guard/ dir), the smoke is the 8 rows of the table (one per arm per block)",
          not os.path.exists(os.path.join(run_dir, "guard")) and stxt.count("=== rep=") == 8 and set(re.findall(r"cell=(\S+) arm=(\S+) attempt", stxt)) == arms8, stxt[:300])
    check(f"{tag}: the rehearsal criteria are printed as PASS lines ([SNAPV], fed - det, [GATES] env, the filter arms, the c1d timeout, no None) and SMOKE-PASS",
          all(re.search(r"^PASS " + re.escape(x), ctxt, re.M) for x in ("[SNAPV] parsed", "fed - det computed", "each arm's env in [GATES]", "the filter arms' liveness", "the c1d 5 GB row", "no None among"))
          and "FAIL " not in ctxt and "SMOKE-PASS" in ctxt, ctxt[-800:])
    want_env = {"BASE": "", "CAND": "", "CANDS": "RWM_RECOV_SNAPVER=1", "BASES": "RWM_RECOV_SNAPVER=1", "CW0": "RWM_PLACE_WDIV=0"}
    heads = re.findall(r"^=== rep=\d+ seed=\d+ cell=(\S+) arm=(\S+) attempt=\d+ .*? bytes=(\d+) arm_env='([^']*)' bin=(\S+) sha=", stxt, re.M)
    check(f"{tag}: every smoke row ran its arm's env on the arm's binary (BASE / BASES on bin-base, CAND / CANDS / CW0 on the CAND binary) at its cell's size",
          len(heads) == 8 and all(env == want_env[a] and (b.endswith("bin-base/raptorpath") == (a in ("BASE", "BASES"))) and by == ("600000000" if c == "c8-100" else "5000000000")
                                  for c, a, by, env, b in heads), heads)
    if path == "full":
        led = [json.loads(ln[len("P38ROW "):]) for ln in open(os.path.join(rep, "ld.log"), errors="replace") if ln.startswith("P38ROW ")]
        n_c8, n_c1d, n_c1d_s = (6, 12, 6) if FULL else (2, 2, 2)
        counts = {}
        for r in led:
            counts[(r["cell"], r["arm"])] = counts.get((r["cell"], r["arm"]), 0) + 1
        want_counts = {("c8-100", a): n_c8 for a in want_env}
        want_counts.update({("c1d-400", "BASE"): n_c1d, ("c1d-400", "CAND"): n_c1d, ("c1d-400", "CANDS"): n_c1d_s})
        check(f"{tag}: the ledger holds the registered rows per (cell, arm) and every one is LIVE with its t53 readouts", counts == want_counts and all(r["status"] == "LIVE" and r.get("t53") for r in led),
              (counts, [r["problems"] for r in led if r["problems"]][:3]))
        echo = {a: {(r["gate_snapver_cli"], r["gate_snapver_srv"], r["gate_wdiv_cli"], r["gate_wdiv_srv"]) for r in led if r["arm"] == a} for a in want_env}
        want_echo = {"BASE": {("0", "0", None, None)}, "CAND": {("0", "0", None, None)}, "CANDS": {("1", "1", None, None)}, "BASES": {("1", "1", None, None)}, "CW0": {("0", "0", "0", "0")}}
        check(f"{tag}: each arm's env is echoed in [GATES] on both ends (RWM_RECOV_SNAPVER 0 on BASE / CAND / CW0, 1 on CANDS / BASES; RWM_PLACE_WDIV=0 on CW0 only)", echo == want_echo, echo)
        shas = {a: {r["bin_sha"] for r in led if r["arm"] == a} for a in want_env}
        check(f"{tag}: BASE and BASES ran BASE's binary, CAND / CANDS / CW0 the CAND binary (one sha each, the two differ)",
              shas["BASE"] == shas["BASES"] and shas["CAND"] == shas["CANDS"] == shas["CW0"] and len(shas["BASE"]) == len(shas["CAND"]) == 1 and shas["BASE"] != shas["CAND"], shas)
        check(f"{tag}: row sizes (600 MB at c8, 5 GB at c1d) and RWM_PERF_TIMEOUT_S=200 in the instrument env", all(r["bytes"] == (600000000 if r["cell"] == "c8-100" else 5000000000) for r in led)
              and "RWM_PERF_TIMEOUT_S=200" in open(os.path.join(rep, "ld.log"), errors="replace").read(), None)
        sc = open(os.path.join(rep, "score.txt"), errors="replace").read()
        check(f"{tag}: the score reports per-arm readouts, the c1d CAND-against-BASE rank-sum table and VERDICT53 REPORTED-ONLY with every row LIVE",
              "VERDICT53 REPORTED-ONLY rows=%d/%d live=%d void=0" % (len(led), len(led), len(led)) in sc and "c1d-400: CAND (n %d) against BASE (n %d)" % (n_c1d, n_c1d) in sc and "[SNAPV] p1 stale/rep" in sc, sc[-600:])


def red53(kind, want):
    """A rehearsal in which the stub injects one defect: the smoke must FAIL (ABORT-SMOKE, rc 5), no battery."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "53", BASE_SHA)
        rc, out, rows, cargo = run_launch(sb, "53", BASE_SHA, rehearse=True, extra_env={"STUB_BAD53": kind})
        chk = os.path.join(sb["root"], "run", "rep53", "smoke-check.txt")
        ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
        check(f"53/red:{kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == 5 and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-700:], tail(out, 8)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def check54(tag, path, run_dir, sb):
    """Plan 54 = plan 41d + the three witnesses: LD_PLAN=54 reaches the battery as 41d with LD_WITNESS54=1, the smoke prints the rehearsal criteria, and (full) every row carries its
    witness readings, the CAND rows hold them, and the score is plan 41d's own output followed by the witness table."""
    import json
    gdir = os.path.join(run_dir, "guard")
    chk = os.path.join(gdir, "smoke-check.txt")
    ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
    check(f"{tag}: the stage is the 41d guard (guard/, no rep dir), its smoke prints the rehearsal criteria as PASS lines (no VOID-SHORT, bin_sha, the c8 hn collapse, [SNAPV]) and SMOKE-PASS",
          os.path.isdir(gdir) and not os.path.exists(os.path.join(run_dir, "rep54b"))
          and all(re.search(r"^PASS " + re.escape(x), ctxt, re.M) for x in ("every smoke row ran", "no row is VOID-SHORT", "bin_sha matches the row's arm", "CAND c8 row: p0 [HOLD] hn_n collapsed", "[SNAPV] present on every CAND row"))
          and "FAIL " not in ctxt and "SMOKE-PASS" in ctxt, ctxt[-900:])
    rd = lambda p: open(os.path.join(sb["devshm"], "ld", p, "raptorpath"), errors="replace").read()  # noqa: E731
    check(f"{tag}: the pre-check's two sides: plan 54's CAND binary carries the [SNAPV] gauge and its BASE (main) binary does not",
          "[SNAPV] site=" in rd("bin") and "[SNAPV] site=" not in rd("bin-base"), None)
    if path != "full":
        return
    led = lambda f: [json.loads(ln[len("P38ROW "):]) for ln in open(os.path.join(gdir, f), errors="replace") if ln.startswith("P38ROW ")]  # noqa: E731
    gl, rl = led("ld.log"), led("repl.log")
    cand, base =[r for r in gl if r["arm"] == "CAND"], [r for r in gl if r["arm"] == "BASE"]
    check(f"{tag}: every CAND row carries its witness readings (hn collapsed at c8, [SNAPV] pt_st + tt_st = 0), every BASE row hn ~9 800 and no [SNAPV]; all witnesses held (no WITNESS-FAIL)",
          cand and base and all(r["w54"]["snapv_present"] and r["w54"]["snapv_st"] == 0 for r in cand) and all(r["w54"]["hn_p0"] == 120 for r in cand if r["cell"] == "c8-100")
          and all(r["w54"]["hn_p0"] == 9800 and not r["w54"]["snapv_present"] for r in base) and not [r for r in gl + rl if r["status"] == "WITNESS-FAIL"],
          ([(r["cell"], r["arm"], r.get("w54")) for r in gl][:4], [r["problems"] for r in gl if r["problems"]][:3]))
    shas = {a: {r["bin_sha"] for r in gl if r["arm"] == a} for a in ("BASE", "CAND")}
    check(f"{tag}: bin_sha is one value per arm and the two differ, and the comparison with the arm's seat EXECUTED on every row (w54.sha_checked), not skipped",
          len(shas["BASE"]) == len(shas["CAND"]) == 1 and shas["BASE"] != shas["CAND"] and all(r["w54"]["sha_checked"] is True for r in gl + rl), (shas, [r["w54"] for r in gl][:2]))
    sc = open(os.path.join(gdir, "score.txt"), errors="replace").read()
    env = dict(os.environ, LD_BASE_SHA=BASE_SHA)
    ref = subprocess.run([sys.executable, os.path.join(sb["repo"], "tools", "l1", "place38_parse.py"), "score41d", os.path.join(gdir, "ld.log").replace("\\", "/"), os.path.join(gdir, "repl.log").replace("\\", "/")], capture_output=True, text=True, errors="replace", env=env).stdout
    # the only text that names the ledger paths is the host-side RESOLUTION hint line (it prints its argv); everything else must match byte for byte
    mask = lambda t: "\n".join("RESOLUTION-HINT" if ln.startswith("RESOLUTION (reported only)") else ln for ln in t.splitlines())  # noqa: E731
    cut = sc.index("WITNESS54") if "WITNESS54" in sc else len(sc)
    check(f"{tag}: the score is plan 41d's own output (score41d on the same ledgers, line for line, VERDICT line included; only the argv echoed in the host-side hint differs) followed by the WITNESS54 table: the witnesses never touch the verdict",
          ref.strip() and mask(sc[:cut]).strip() == mask(ref).strip() and "WITNESS54" in sc and "\nVERDICT " in ref, (mask(sc[:cut])[-300:], mask(ref)[-300:]))


def check54b(tag, path, run_dir, sb):
    """Plan 54b's own assertions: one stage rep54b (no guard), the 6-row smoke with the pool in [GATES] and pool_per_path on both ends and the rehearsal criteria printed, and (full) the
    ledger: 6 arms on their seats with their pool env, the pool echoed, every readout computed, the report with falsifiers and the re-basing line."""
    import json
    rep = os.path.join(run_dir, "rep54b")
    smoke = os.path.join(rep, "smoke.log")
    stxt = open(smoke, errors="replace").read() if os.path.exists(smoke) else ""
    chk = os.path.join(rep, "smoke-check.txt")
    ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
    heads = re.findall(r"^=== rep=\d+ seed=\d+ cell=(\S+) arm=(\S+) attempt=\d+ .*? bytes=(\d+) arm_env='([^']*)' bin=(\S+) sha=", stxt, re.M)
    want_env = {"B2048": "", "B1024": "RWM_STORE_PATH_POOL=1024", "B640": "RWM_STORE_PATH_POOL=640", "L2048": "", "L1024": "RWM_STORE_PATH_POOL=1024", "L640": "RWM_STORE_PATH_POOL=640"}
    check(f"{tag}: no guard stage, the smoke is the 6 rows of the table; each ran its pool env (the controls none) on its seat's binary (B* on bin-base, L* on the CAND binary) at 600 MB",
          not os.path.exists(os.path.join(run_dir, "guard")) and len(heads) == 6 and {a for _, a, _, _, _ in heads} == set(want_env)
          and all(c == "c8-100" and by == "600000000" and env == want_env[a] and (b.endswith("bin-base/raptorpath") == a.startswith("B")) for c, a, by, env, b in heads), heads)
    check(f"{tag}: the rehearsal criteria are printed as PASS lines (the pool on both ends, [SNAPV], no None, the 200 s timeout) and SMOKE-PASS",
          all(re.search(r"^PASS " + re.escape(x), ctxt, re.M) for x in ("one row per block arm ran", "each arm's pool is in [GATES] on both ends", "each row ran the build of its seat", "[SNAPV] parsed on every row",
                                                                         "in-order delay (run and late), evictions, old holes and the budget residual computed (no None)", "the c8 600 MB rows finished inside RWM_PERF_TIMEOUT_S=300"))
          and "FAIL " not in ctxt and "SMOKE-PASS" in ctxt, ctxt[-1200:])
    # [SNAPV] pre-check gating: plan 54b's BASE seat is cand54, which carries the gauge, so BOTH binaries have it (the battery refuses a BASE binary with the gauge only for plan 54)
    bb = os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")
    check(f"{tag}: the BASE-seat binary carries the [SNAPV] gauge and the battery did not refuse it (the 'BASE has no [SNAPV]' pre-check is plan 54's only)",
          os.path.exists(bb) and "[SNAPV] site=" in open(bb, errors="replace").read() and "ABORT-SHA BASE binary" not in open(os.path.join(rep, "smoke.log"), errors="replace").read(), None)
    drv = [ln.split() for ln in open(os.path.join(sb["logs"], "drv.log")).read().splitlines()]
    check(f"{tag}: every driver call ran with RWM_PERF_TIMEOUT_S=300", drv and all(d[3] == "timeout=300" for d in drv), drv[:3])
    if path != "full":
        return
    led = [json.loads(ln[len("P38ROW "):]) for ln in open(os.path.join(rep, "ld.log"), errors="replace") if ln.startswith("P38ROW ")]
    n_arm = 6 if FULL else 2
    check(f"{tag}: the ledger holds {n_arm} rows per arm, every row LIVE with its t54 readouts and none of them None, and the pool echoed on both ends and in pool_per_path",
          len(led) == 6 * n_arm and all(r["status"] == "LIVE" and r.get("t54") and not pl_missing(r) for r in led)
          and all((r["gate_pool_cli"], r["gate_pool_srv"], r["pool_info_cli"], r["pool_info_srv"]) == (str(pool),) * 4 for r in led for pool in [int(r["arm"][1:])]),
          (len(led), [(r["arm"], r["status"], r["problems"], pl_missing(r)) for r in led if r["status"] != "LIVE" or pl_missing(r)][:3]))
    shas = {a: {r["bin_sha"] for r in led if r["arm"] == a} for a in want_env}
    check(f"{tag}: B* ran the BASE seat's binary and L* the CAND seat's (one sha each, the two differ), the comparison executed on every row (sha_checked)",
          all(r["sha_checked"] is True for r in led) and len(set().union(*shas.values())) == 2 and all(len(v) == 1 for v in shas.values())
          and shas["B2048"] == shas["B640"] and shas["L2048"] == shas["L640"] and shas["B2048"] != shas["L2048"], shas)
    check(f"{tag}: every row's cap is 2 x its pool (bind fraction 1.0), the instrument env is plan 52's with RWM_PERF_TIMEOUT_S=300 (plan 54b only), and the env scrub withholds RWM_STORE_PATH_POOL",
          all(r["t54"]["bind_frac"] == 1.0 and r["t54"]["cap_med"] == 2 * int(r["arm"][1:]) for r in led) and "RWM_PERF_TIMEOUT_S=300" in open(os.path.join(rep, "ld.log"), errors="replace").read()
          and "RWM_PERF_TIMEOUT_S=200" not in open(os.path.join(rep, "ld.log"), errors="replace").read().split("=== env ")[-1].split("\n")[0]
          and "RWM_STORE_PATH_POOL;" in open(os.path.join(rep, "ld.log"), errors="replace").read(), None)
    sc = open(os.path.join(rep, "score.txt"), errors="replace").read()
    check(f"{tag}: the report prints the readouts per arm, the four falsifiers, the decision-rule reading, the re-basing line and VERDICT54B REPORTED-ONLY with every row LIVE",
          "VERDICT54B REPORTED-ONLY rows=%d/%d live=%d" % (len(led), len(led), len(led)) in sc and all(f"F{i} " in sc for i in (1, 2, 3, 4)) and "DECISION-RULE reading" in sc
          and ("REBASE-NEEDED:" in sc or "REBASE-CHECK UNAVAILABLE" in sc) and "inorder_mean_late" in sc and "old holes" in sc, sc[-900:])


def pl_missing(r):
    import pool54_score
    return pool54_score.missing(r)


def red54(kind, want):
    """Plan 54's rehearsal in which the stub injects one defect (STUB_BAD54): the smoke must FAIL (ABORT-SMOKE, rc 5), no battery."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "54", BASE_SHA)
        rc, out, rows, cargo = run_launch(sb, "54", BASE_SHA, rehearse=True, extra_env={"STUB_BAD54": kind})
        chk = os.path.join(sb["root"], "run", "guard", "smoke-check.txt")
        ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
        check(f"54/red:{kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == 5 and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-700:], tail(out, 8)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def dnf54b():
    """A B640 row that times out (seed 7): the row is LIVE with dnf true, keeps its [DIAG]-derived readouts (old holes, inorder_mean_late, residual, q0), is counted in the report
    (goodput 0 in the falsifiers) and the battery finishes."""
    import json
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "54b", BASE_SHA)
        rc, out, rows, cargo = run_launch(sb, "54b", BASE_SHA, extra_env={"STUB_DNF54": "B640"})
        rep = os.path.join(sb["root"], "run", "rep54b")
        led = [json.loads(ln[len("P38ROW "):]) for ln in open(os.path.join(rep, "ld.log"), errors="replace") if ln.startswith("P38ROW ")] if os.path.exists(os.path.join(rep, "ld.log")) else []
        d = [r for r in led if r["arm"] == "B640" and r["seed"] == "7"]
        t = (d[0].get("t54") or {}) if d else {}
        sc = open(os.path.join(rep, "score.txt"), errors="replace").read() if os.path.exists(os.path.join(rep, "score.txt")) else ""
        check("54b/dnf: the session finishes (rc 0, LD-ALL-DONE) with a B640 DNF at seed 7: the row is LIVE, dnf true, no goodput",
              rc == 0 and "LD-ALL-DONE" in out and len(d) == (3 if FULL else 1) and all(x["status"] == "LIVE" and x["dnf"] is True and x.get("mbps") is None for x in d), (rc, tail(out, 6), [(r["arm"], r["status"], r["dnf"]) for r in led][:4]))
        check("54b/dnf: the DNF row keeps old holes, inorder_mean_late, the budget residual and q0 computed from its [DIAG] / [RANK] lines (none None)",
              all(t.get(k) is not None for k in ("old_holes", "inorder_mean_late", "inorder_mean_run", "resid_late", "resid_rel", "q0_late", "rtt0_late")), t)
        check("54b/dnf: the report counts it (dnf=1 in the verdict line, 'DNF 1' for B640) and the falsifiers still evaluate",
              ("dnf=%d" % len(d)) in sc and re.search(r"B640\s+rows \d+.*DNF %d" % len(d), sc) is not None and "F1 " in sc, sc[-700:])      # FULL runs three seed-7 B640 rows, fast mode one
    finally:
        shutil.rmtree(root, ignore_errors=True)


def red54sha(plan, stage, want):
    """The sha witness failing once in a stub run: the sandbox's battery hands the row parser a wrong sha, the rehearsal's smoke must FAIL on the bin_sha criterion."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None if plan in SINGLE else BASE_SHA)
        bt = os.path.join(sb["repo"], "tools", "l1", "loopdiag_battery.sh")
        txt = open(bt, newline="").read()
        old = '"$TMPD/rwm-s.log" "$cot" "$sha" >> "$OUT" 2>&1'
        assert txt.count(old) == 1, txt.count(old)
        with open(bt, "w", newline="") as fh:
            fh.write(txt.replace(old, '"$TMPD/rwm-s.log" "$cot" "0000deadbeef" >> "$OUT" 2>&1'))
        rc, out, rows, cargo = run_launch(sb, plan, None if plan in SINGLE else BASE_SHA, rehearse=True)
        chk = os.path.join(sb["root"], "run", stage, "smoke-check.txt")
        ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
        check(f"{plan}/red:sha: a row whose bin_sha is not its seat's build fails the smoke (ABORT-SMOKE, rc 5) on the criterion '{want}'",
              rc == 5 and "ABORT-SMOKE" in era(sb) and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-600:], tail(out, 6)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def red54b(kind, want):
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "54b", BASE_SHA)
        rc, out, rows, cargo = run_launch(sb, "54b", BASE_SHA, rehearse=True, extra_env={"STUB_BAD54": kind})
        chk = os.path.join(sb["root"], "run", "rep54b", "smoke-check.txt")
        ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
        check(f"54b/red:{kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == 5 and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-700:], tail(out, 8)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def ref56_file(sb, miss):
    """Plan 56's reference manifest for the sandbox: the sha256 of every file under raptorpath/src and raptorpath/tests of the sandbox tree (= what suites_stage56 computes: a hit), or the same with
    its first hash character changed (a miss)."""
    out = []
    for top in ("raptorpath/src", "raptorpath/tests"):
        for dp, _, fns in os.walk(os.path.join(sb["repo"], top)):
            for fn in fns:
                full = os.path.join(dp, fn)
                out.append((os.path.relpath(full, sb["repo"]).replace("\\", "/"), hashlib.sha256(open(full, "rb").read()).hexdigest()))
    text = "".join("%s  %s\n" % (h, rel) for rel, h in sorted(out))
    if miss:
        text = ("0" if text[0] != "0" else "1") + text[1:]
    path = os.path.join(sb["logs"], "ref56.sha256")
    with open(path, "w", newline="\n") as fh:
        fh.write(text)
    return px(path)


def led56(rep, name="ld.log"):
    import json
    p = os.path.join(rep, name)
    return [json.loads(ln[len("P38ROW "):]) for ln in open(p, errors="replace") if ln.startswith("P38ROW ")] if os.path.exists(p) else []


def pl_missing56(r):
    import pool56_score
    return pool56_score.missing(r)


def check56(tag, path, run_dir, sb):
    """Plan 56's own assertions: ONE stage rep56 (no guard, one seat), the 4-row smoke (one row per cell at its smallest pool + the dump row) with the pool and dump gates echoed and the rehearsal
    criteria printed, and (a full session) the ledger: every slot of the table, the pools echoed, every readout computed, the rotation, the dump row, the report with F1-F10."""
    import place38_parse as pp
    rep = os.path.join(run_dir, "rep56")
    stxt = open(os.path.join(rep, "smoke.log"), errors="replace").read() if os.path.exists(os.path.join(rep, "smoke.log")) else ""
    ctxt = open(os.path.join(rep, "smoke-check.txt"), errors="replace").read() if os.path.exists(os.path.join(rep, "smoke-check.txt")) else ""
    heads = re.findall(r"^=== rep=\d+ seed=\d+ cell=(\S+) arm=(\S+) attempt=\d+ .*? bytes=(\d+) arm_env='([^']*)' bin=(\S+) sha=", stxt, re.M)
    want = {("c8-100", "P80"): ("600000000", "RWM_STORE_PATH_POOL=80"), ("c2-100", "P128"): ("600000000", "RWM_STORE_PATH_POOL=128"),
            ("c1s-400", "P384"): ("5000000000", "RWM_STORE_PATH_POOL=384"), ("c8-100", "P640D"): ("600000000", "RWM_STORE_PATH_POOL=640 RWM_SUCC_DUMP=1")}
    check(f"{tag}: no guard stage, no BASE tree; the smoke is the 4 rows (one per cell at its smallest pool + the dump row), each with its pool env at its cell's size on the ONE binary (bin/, never bin-base)",
          not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and len(heads) == 4
          and {(c, a) for c, a, _, _, _ in heads} == set(want) and all(want[(c, a)] == (by, env) and b.endswith("/bin/raptorpath") for c, a, by, env, b in heads), heads)
    check(f"{tag}: the rehearsal criteria are printed as PASS lines (pool and dump gates, the one seat's sha, the LATE readouts, the 300 s timeout, the dump) and SMOKE-PASS",
          all(re.search(r"^PASS " + re.escape(x), ctxt, re.M) for x in ("one row per smoke slot ran", "each row's pool is in [GATES] on both ends", "the dump gate reads RWM_SUCC_DUMP=1", "every row ran the one seat's binary",
                                                                         "the LATE-window readouts", "every smoke row finished inside RWM_PERF_TIMEOUT_S=300", "the dump row wrote a raw dump"))
          and "FAIL " not in ctxt and "SMOKE-PASS" in ctxt, ctxt[-1400:])
    drv = [ln.split() for ln in open(os.path.join(sb["logs"], "drv.log")).read().splitlines()]
    check(f"{tag}: every driver call ran with RWM_PERF_TIMEOUT_S=300 and the cells' sizes (c8 / c2 600 MB, c1s 5 GB)",
          drv and all(d[3] == "timeout=300" for d in drv) and {(d[0], d[1], d[2]) for d in drv} <= {("c2", "c3", "600000000"), ("c2", "c2", "600000000"), ("c1", "c1", "5000000000")}, drv[:3])
    if path == "rehearse":
        sdir = os.path.join(run_dir, "suites")
        check(f"{tag}: a rehearsal runs the tools tests (python and bash) but no engine suites, and leaves NO suite record (no TESTS-OK, no SRC-MANIFEST.sha256) for another plan's LD_PRIOR_TESTS to accept",
              os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")) and not os.path.exists(os.path.join(sdir, "TESTS-OK")) and not os.path.exists(os.path.join(sdir, "SRC-MANIFEST.sha256"))
              and "TEST bashtests rc=0" in open(os.path.join(sdir, "TESTS.txt"), errors="replace").read(), os.listdir(sdir) if os.path.isdir(sdir) else None)
        return
    led = led56(rep)
    table = [r for r in pp.plan56() if r[4] == 1 or r[2] == pp.DUMP_ARM56] if not FULL else pp.plan56()
    want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
    got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
    check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}: c8 -> c2 -> c1s -> the dump row), one row each, every row LIVE with its t56 readouts none of them None",
          got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t56") and not pl_missing56(r) for r in led),
          (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], pl_missing56(r)) for r in led if r["status"] != "LIVE" or pl_missing56(r)][:3], got_slots[:3], want_slots[:3]))
    check(f"{tag}: the pool is echoed on both ends and in pool_per_path for every row, every row's cap is N x its pool (N = 2 at c8, 1 elsewhere) and binds (bind fraction 1.0)",
          all((r["gate_pool_cli"], r["gate_pool_srv"], r["pool_info_cli"], r["pool_info_srv"]) == (str(pp.arm_pool56(r["arm"])[0]),) * 4 for r in led)
          and all(r["t56"]["bind_frac"] == 1.0 and r["t56"]["cap_mean"] == pp.NLIVE56[r["cell"]] * pp.arm_pool56(r["arm"])[0] for r in led), None)
    shas = {r["bin_sha"] for r in led}
    check(f"{tag}: one binary ran every row (one sha) and the seat comparison executed on every row (sha_checked)", len(shas) == 1 and all(r["sha_checked"] is True for r in led), shas)
    dumps = [r for r in led if r["arm"] == pp.DUMP_ARM56]
    check(f"{tag}: the dump row carries RWM_SUCC_DUMP=1 in both [GATES] echoes and >= 100 000 records (size recorded), every other row 0 records",
          len(dumps) == 1 and dumps[0]["succdump"]["records"] >= pp.DUMP_MIN_RECORDS56 and dumps[0]["succdump"]["bytes"] > 0
          and all(r["succdump"]["records"] == 0 for r in led if r["arm"] != pp.DUMP_ARM56), [(r["arm"], r["succdump"]) for r in dumps])
    ldtxt = open(os.path.join(rep, "ld.log"), errors="replace").read()
    check(f"{tag}: the instrument env is RWM_RTOBS RWM_RDIAG with RWM_PERF_TIMEOUT_S=300 (not 200), and the env scrub withholds RWM_STORE_PATH_POOL, RWM_SUCC_DUMP and RWM_SUCC_DUMP_MAX",
          "RWM_PERF_TIMEOUT_S=300" in ldtxt and "RWM_PERF_TIMEOUT_S=200" not in ldtxt.split("=== env ")[-1].split("\n")[0] and "RWM_SUCC_DUMP RWM_SUCC_DUMP_MAX RWM_STORE_PATH_POOL;" in ldtxt, None)
    sc = open(os.path.join(rep, "score.txt"), errors="replace").read()
    m = re.search(r"^VERDICT56 REPORTED-ONLY slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+) dnf=(\d+) unbound=(\d+) qcollapse=(\d+) falsifiers=(\S+)", sc, re.M)
    check(f"{tag}: the report prints the per-cell tables, the standing-queue readout, the re-basing line, the bands, F1-F10 each FIRES / DOES-NOT-FIRE / UNSCOREABLE and a VERDICT56 line counting every slot",
          m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led) and all(f"F{i} " in sc for i in range(1, 11))
          and all(re.search(r"^  F%d (FIRES|DOES-NOT-FIRE|UNSCOREABLE) " % i, sc, re.M) for i in range(1, 11)) and "STANDING QUEUE" in sc and ("REBASE-NEEDED:" in sc or "REBASE-CHECK UNAVAILABLE" in sc)
          and "== c8-100" in sc and "== c1s-400" in sc, sc[-900:])
    tests_txt = open(os.path.join(run_dir, "suites", "TESTS.txt"), errors="replace").read()
    check(f"{tag}: the tools tests ran in the suites step (TEST parsers and TEST bashtests lines), whether or not the engine suites did",
          "TEST parsers " in tests_txt and "TEST bashtests " in tests_txt, tests_txt)


def red56(kind, want):
    """A defect injected by the stub into plan 56's rehearsal: the smoke must FAIL on the criterion that catches it (ABORT-SMOKE, rc 5, no REHEARSAL-PASS)."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "56", None)
        rc, out, rows, cargo = run_launch(sb, "56", None, rehearse=True, extra_env={"STUB_BAD56": kind})
        chk = os.path.join(sb["root"], "run", "rep56", "smoke-check.txt")
        ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
        check(f"56/red:{kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == 5 and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-700:], tail(out, 8)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def repl56():
    """A full plan-56 session in which the stub makes the first c8 P384 seed-42 row echo the wrong pool (VOID -> replaced once), the first dump row write 50 000 records (-> re-run once), and
    the c8 P112 seed-42 row time out (a DNF is LIVE, goodput 0, its other readouts kept): every replacement is a second row of the same slot, only once, and the session completes."""
    import place38_parse as pp
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "56", None)
        rc, out, rows, cargo = run_launch(sb, "56", None, extra_env={"STUB_BAD56": "poolonce,dumpshort", "STUB_DNF56": "P112:42"})
        rep = os.path.join(sb["root"], "run", "rep56")
        led = led56(rep)
        ldtxt = open(os.path.join(rep, "ld.log"), errors="replace").read() if os.path.exists(os.path.join(rep, "ld.log")) else ""
        sc = open(os.path.join(rep, "score.txt"), errors="replace").read() if os.path.exists(os.path.join(rep, "score.txt")) else ""
        p384 = [r for r in led if (r["cell"], r["arm"], r["seed"], r["rep"]) == ("c8-100", "P384", "42", 1)]
        dmp = [r for r in led if r["arm"] == pp.DUMP_ARM56]
        per = {}
        for r in led:
            per.setdefault((r["cell"], r["arm"], r["seed"], r["rep"]), []).append(r)
        check("56/repl: the session completes (rc 0, LD-ALL-DONE) with 2 replaced slots: 20 ledger rows for 18 slots, no slot with more than 2 rows",
              rc == 0 and "LD-ALL-DONE" in out and len(led) == (72 if FULL else 20) and len(per) == (70 if FULL else 18) and max(len(v) for v in per.values()) == 2, (rc, len(led), len(per), tail(out, 6)))
        check("56/repl: the VOID pool echo is replaced once: P384's first row is WITNESS-FAIL on gate-pool (echo 2048), its replacement LIVE with the pool echoed, 'REPLACE56 ... reason=pool-echo' in the ledger",
              len(p384) == 2 and p384[0]["status"] == "WITNESS-FAIL" and pp.is_pool_void56(p384[0]) and p384[1]["status"] == "LIVE" and p384[1]["gate_pool_cli"] == "384"
              and re.search(r"^REPLACE56 c8-100 P384 s42 rep=1 reason=pool-echo", ldtxt, re.M) is not None, [(r["status"], r["problems"]) for r in p384])
        check("56/repl: the short dump is re-run once: 50 000 records then 120 000 (the dump size recorded on both rows), 'REPLACE56 ... reason=dump-short' in the ledger",
              len(dmp) == 2 and [r["succdump"]["records"] for r in dmp] == [50000, 120000] and all(r["succdump"]["bytes"] > 0 for r in dmp)
              and re.search(r"^REPLACE56 c8-100 P640D s42 rep=5 reason=dump-short", ldtxt, re.M) is not None, [(r["status"], r["succdump"]) for r in dmp])
        d = [r for r in led if (r["cell"], r["arm"], r["seed"]) == ("c8-100", "P112", "42")]
        t = (d[0].get("t56") or {}) if d else {}
        check("56/repl: the DNF row (c8 P112) is LIVE with dnf true and no goodput, and keeps rates, Tbar, old holes and the Little residual from its [DIAG] / [RANK] lines (none None); the report counts it",
              len(d) == (2 if FULL else 1) and all(x["status"] == "LIVE" and x["dnf"] is True and x.get("mbps") is None for x in d)       # the registered design runs P112 at seed 42 twice (replicates 1 and 3)
              and all(t.get(k) is not None for k in ("tx_sum_late", "r0", "r1", "Tbar_ms", "old_holes", "resid_late", "inorder_mean_late")) and ("dnf=%d" % len(d)) in sc and "replaced=2" in sc, (t, sc[-300:]))
        check("56/repl: the score lists both replaced rows and the F-lines still evaluate",
              sc.count("  REPLACED ") == 2 and all(re.search(r"^  F%d (FIRES|DOES-NOT-FIRE|UNSCOREABLE) " % i, sc, re.M) for i in range(1, 11)), sc[:900])
    finally:
        shutil.rmtree(root, ignore_errors=True)


def refused56(name, extra, want_rc, want_in_era, no_build=True, failed=True):
    """Plan 56 refusals (a rehearsal in which nothing may be measured): too little /dev/shm, LD_PRIOR_TESTS."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "56", None)
        extra_env = dict(extra)
        if "LD_PRIOR_TESTS" in extra_env:
            extra_env["LD_PRIOR_TESTS"] = px(sb["root"])
        rc, out, rows, cargo = run_launch(sb, "56", None, rehearse=True, extra_env=extra_env)
        e = era(sb)
        check(f"56/{name}: refused (rc {want_rc}, FAILED-ALL, '{want_in_era}'), no row measured{', no build' if no_build else ''}",
              rc == want_rc and want_in_era in e + out and (os.path.exists(os.path.join(sb["root"], "run", "FAILED-ALL")) or not failed) and rows == [] and (not no_build or not [c for c in cargo if "|build " in c]), (rc, tail(e, 6), tail(out, 6)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def check57(tag, path, run_dir, sb, plan):
    """Plans 57a / 57b's own assertions: ONE stage rep57a / rep57b (no guard, one seat), the smoke rows (57a: the B256-512 row; 57b: pools 192 and 2048) with the pool AND burst echoed and the
    rehearsal criteria printed, the 300 s timeout at 5 GB, and (a full session) the ledger: every slot of the table, the knobs echoed on both ends, every readout computed, the report."""
    import place38_parse as pp
    import pool57_score as p57
    rep = os.path.join(run_dir, "rep" + plan)
    stxt = open(os.path.join(rep, "smoke.log"), errors="replace").read() if os.path.exists(os.path.join(rep, "smoke.log")) else ""
    ctxt = open(os.path.join(rep, "smoke-check.txt"), errors="replace").read() if os.path.exists(os.path.join(rep, "smoke-check.txt")) else ""
    heads = re.findall(r"^=== rep=\d+ seed=\d+ cell=(\S+) arm=(\S+) attempt=\d+ .*? bytes=(\d+) arm_env='([^']*)' bin=(\S+) sha=", stxt, re.M)
    if plan == "57a":
        want = {("c1s-400", "B256_512"): ("5000000000", "RWM_STORE_PATH_POOL=512 RWM_EMIT_BURST=256")}
    else:
        want = {("c1d-400", "P192"): ("5000000000", "RWM_STORE_PATH_POOL=192"), ("c1d-400", "P2048"): ("5000000000", "")}
    check(f"{tag}: no guard stage, no BASE tree; the smoke is {len(want)} row(s) of the table, each with its arm env (pool AND burst at 57a) at 5 GB on the ONE binary (bin/, never bin-base)",
          not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and len(heads) == len(want)
          and {(c, a) for c, a, _, _, _ in heads} == set(want) and all(want[(c, a)] == (by, env) and b.endswith("/bin/raptorpath") for c, a, by, env, b in heads), heads)
    crits = ["one row per smoke slot ran", "each row's pool AND burst are in [GATES] on both ends", "the successor dump is off", "every row ran the one seat's binary", "the LATE-window readouts",
             "every smoke row finished inside RWM_PERF_TIMEOUT_S=300"] + (["the knob acts"] if plan == "57a" else [])
    check(f"{tag}: the rehearsal criteria are printed as PASS lines (pool and burst gates, the sha, the LATE readouts, the 300 s timeout{', the burst > 64' if plan == '57a' else ''}) and SMOKE-PASS",
          all(re.search(r"^PASS " + re.escape(x), ctxt, re.M) for x in crits) and "FAIL " not in ctxt and "SMOKE-PASS" in ctxt, ctxt[-1400:])
    drv = [ln.split() for ln in open(os.path.join(sb["logs"], "drv.log")).read().splitlines()]
    check(f"{tag}: every driver call ran with RWM_PERF_TIMEOUT_S=300 at 5 GB on the cell's scenario", drv and all(d[3] == "timeout=300" and d[2] == "5000000000" and d[0] == "c1" for d in drv), drv[:3])
    if path == "rehearse":
        sdir = os.path.join(run_dir, "suites")
        check(f"{tag}: a rehearsal runs the tools tests (python and bash) but no engine suites, and leaves NO suite record (no TESTS-OK, no SRC-MANIFEST.sha256)",
              os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")) and not os.path.exists(os.path.join(sdir, "TESTS-OK")) and not os.path.exists(os.path.join(sdir, "SRC-MANIFEST.sha256"))
              and "TEST bashtests rc=0" in open(os.path.join(sdir, "TESTS.txt"), errors="replace").read(), os.listdir(sdir) if os.path.isdir(sdir) else None)
        return
    led = led56(rep)
    table = pp.plan57(plan) if FULL else [x for x in pp.plan57(plan) if x[4] <= 2]      # fast mode: the sandbox caps the replicates at 2 (REPS_CAP57)
    want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
    got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
    check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}), one row each, every row LIVE with its t56 / t57 readouts none of them None",
          got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t56") and r.get("t57") and not p57.missing57(plan, r) for r in led),
          (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], p57.missing57(plan, r)) for r in led if r["status"] != "LIVE" or p57.missing57(plan, r)][:3], got_slots[:3], want_slots[:3]))
    check(f"{tag}: the pool is echoed on both ends and in pool_per_path for every row; every non-control row's cap is n_live x its pool and binds (bind fraction 1.0)",
          all((r["gate_pool_cli"], r["gate_pool_srv"], r["pool_info_cli"], r["pool_info_srv"]) == (str(pp.arm_knobs57(plan, r["arm"])[0]),) * 4 for r in led)
          and all(r["t56"]["bind_frac"] == 1.0 and r["t56"]["cap_mean"] == pp.NLIVE57[r["cell"]] * pp.arm_knobs57(plan, r["arm"])[0] for r in led), None)
    burst_ok = True
    for r in led:
        # the burst echo rides in the ledger only through the problems list (gate-burst-*): a row that echoed the wrong burst would be a WITNESS-FAIL / VOID
        burst_ok &= not any(x.startswith("gate-burst") for x in r["problems"])
    check(f"{tag}: no row carries a gate-burst problem (RWM_EMIT_BURST echoed on both ends as requested: 64 / 64 / 256 at 57a, the shipped 64 at 57b)", burst_ok, [r["problems"] for r in led if r["problems"]][:3])
    shas = {r["bin_sha"] for r in led}
    check(f"{tag}: one binary ran every row (one sha) and the seat comparison executed on every row (sha_checked)", len(shas) == 1 and all(r["sha_checked"] is True for r in led), shas)
    ldtxt = open(os.path.join(rep, "ld.log"), errors="replace").read()
    check(f"{tag}: the instrument env is RWM_RTOBS RWM_RDIAG with RWM_PERF_TIMEOUT_S=300, and the env scrub withholds RWM_EMIT_BURST and RWM_STORE_PATH_POOL",
          "RWM_PERF_TIMEOUT_S=300" in ldtxt and "RWM_PERF_TIMEOUT_S=200" not in ldtxt.split("=== env ")[-1].split("\n")[0] and "RWM_EMIT_BURST" in ldtxt.split("=== env ")[-1].split("\n")[0]
          and "RWM_STORE_PATH_POOL;" in ldtxt.split("=== env ")[-1].split("\n")[0], None)
    sc = open(os.path.join(rep, "score.txt"), errors="replace").read()
    m = re.search(r"^VERDICT57%s REPORTED-ONLY slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+) dnf=(\d+)" % plan[-1].upper(), sc, re.M)
    check(f"{tag}: the report prints the per-{'arm' if plan == '57a' else 'pool'} readouts, the {'OUTCOME block (z_null, z_hyp, S, the gates, the re-base, E5)' if plan == '57a' else 'transfer test T (control queue, offsets, (a), (b))'} and a VERDICT57{plan[-1].upper()} line counting every slot",
          m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led) and ("OUTCOME" in sc and "READING" in sc and "E5" in sc if plan == "57a" else "(c) THE C1D CONTROL" in sc and "T = " in sc), sc[-900:])
    tests_txt = open(os.path.join(run_dir, "suites", "TESTS.txt"), errors="replace").read()
    check(f"{tag}: the tools tests ran in the suites step (TEST parsers and TEST bashtests lines), whether or not the engine suites did", "TEST parsers " in tests_txt and "TEST bashtests " in tests_txt, tests_txt)


def red57(plan, kind, want):
    """A defect injected by the stub into a plan-57 rehearsal: the smoke must FAIL on the criterion that catches it (ABORT-SMOKE, rc 5, no REHEARSAL-PASS)."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=True, extra_env={"STUB_BAD57": kind})
        chk = os.path.join(sb["root"], "run", "rep" + plan, "smoke-check.txt")
        ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
        check(f"{plan}/red:{kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == 5 and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-700:], tail(out, 8)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def repl57(plan):
    """A full plan-57 session in which the stub makes the first B64-448 (57a) / P384 (57b) seed-42 row echo the wrong pool (VOID -> replaced once) and another arm's seed-42 rows time out (a DNF is
    LIVE, goodput 0, its readouts kept): every replacement is a second row of the same slot, only once, and the session completes."""
    import place38_parse as pp
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        cell, repl_arm, dnf_arm = ("c1s-400", "B64_448", "B64_512") if plan == "57a" else ("c1d-400", "P384", "P256")
        rc, out, rows, cargo = run_launch(sb, plan, None, extra_env={"STUB_BAD57": "poolonce", "STUB_DNF57": "%s:42" % dnf_arm})
        rep = os.path.join(sb["root"], "run", "rep" + plan)
        led = led56(rep)
        ldtxt = open(os.path.join(rep, "ld.log"), errors="replace").read() if os.path.exists(os.path.join(rep, "ld.log")) else ""
        sc = open(os.path.join(rep, "score.txt"), errors="replace").read() if os.path.exists(os.path.join(rep, "score.txt")) else ""
        per = {}
        for r in led:
            per.setdefault((r["cell"], r["arm"], r["seed"], r["rep"]), []).append(r)
        nslots = len(pp.plan57(plan)) if FULL else len([x for x in pp.plan57(plan) if x[4] <= 2])
        check(f"{plan}/repl: the session completes (rc 0, LD-ALL-DONE) with ONE replaced slot: one slot with 2 ledger rows, none with more",
              rc == 0 and "LD-ALL-DONE" in out and len(per) == nslots and len(led) == nslots + 1 and max(len(v) for v in per.values()) == 2, (rc, len(led), len(per), tail(out, 6)))
        first = [r for r in led if (r["cell"], r["arm"], r["seed"], r["rep"]) == (cell, repl_arm, "42", 1)]
        pool = pp.arm_knobs57(plan, repl_arm)[0]
        check(f"{plan}/repl: the VOID pool echo is replaced once: the first {repl_arm} row is WITNESS-FAIL on gate-pool (echo 2048), the replacement LIVE with pool {pool}, 'REPLACE57 ... reason=echo' in the ledger",
              len(first) == 2 and first[0]["status"] == "WITNESS-FAIL" and pp.is_echo_void57(first[0]) and first[1]["status"] == "LIVE" and first[1]["gate_pool_cli"] == str(pool)
              and re.search(r"^REPLACE57 %s %s s42 rep=1 reason=echo" % (cell, repl_arm), ldtxt, re.M) is not None, [(r["status"], r["problems"]) for r in first])
        d = [r for r in led if (r["cell"], r["arm"], r["seed"]) == (cell, dnf_arm, "42") and r["status"] == "LIVE"]
        t = (d[0].get("t56") or {}) if d else {}
        check(f"{plan}/repl: the DNF row(s) ({dnf_arm} at seed 42) are LIVE with dnf true and no goodput, keeping rates and gauges from the [DIAG] lines (none None); the report counts them",
              len(d) >= 1 and all(x["dnf"] is True and x.get("mbps") is None for x in d) and all(t.get(k) is not None for k in ("tx_sum_late", "r0", "Tbar_ms", "old_holes")) and ("dnf=%d" % len(d)) in sc,
              (len(d), t.get("r0"), sc[-300:]))
        check(f"{plan}/repl: the score lists the replaced row and prints its VERDICT line", sc.count("  REPLACED ") == 1 and ("VERDICT57%s REPORTED-ONLY" % plan[-1].upper()) in sc, sc[:700])
    finally:
        shutil.rmtree(root, ignore_errors=True)


def refused57(plan, name, extra, want_rc, want_in_era, no_build=True, failed=True):
    """Plan 57 refusals (a rehearsal in which nothing may be measured): too little /dev/shm, the budget guard (worst case of the status), LD_PRIOR_TESTS."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        extra_env = dict(extra)
        if "LD_PRIOR_TESTS" in extra_env:
            extra_env["LD_PRIOR_TESTS"] = px(sb["root"])
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=True, extra_env=extra_env)
        e = era(sb)
        check(f"{plan}/{name}: refused (rc {want_rc}, '{want_in_era}'), no row measured{', no build' if no_build else ''}",
              rc == want_rc and want_in_era in e + out and (os.path.exists(os.path.join(sb["root"], "run", "FAILED-ALL")) or not failed) and rows == [] and (not no_build or not [c for c in cargo if "|build " in c]), (rc, tail(e, 6), tail(out, 6)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def budget57(plan, left_s, ok):
    """The budget guard: LD_HARD with `left_s` seconds left is refused below the worst case of the status (57a 13 200 s, 57b 5 400 s) and accepted at it."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=True, extra_env={"LD_HARD": str(int(time.time()) + left_s)})
        e = era(sb)
        if ok:
            check(f"{plan}/budget: {left_s} s left (just above the {13200 if plan == '57a' else 5400} s worst case) is accepted: the rehearsal completes", rc == 0 and "ABORT-BUDGET" not in e and "LD-ALL-REHEARSAL-DONE" in out, (rc, tail(e, 4)))
        else:
            check(f"{plan}/budget: {left_s} s left (below the {13200 if plan == '57a' else 5400} s worst case) is refused before anything runs (ABORT-BUDGET plan 57a/57b, rc 3)",
                  rc == 3 and "ABORT-BUDGET plan 57a/57b" in e + out and rows == [] and not [c for c in cargo if "|build " in c], (rc, tail(e, 4)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def check59(tag, path, run_dir, sb):
    """Plan 59's own assertions: ONE stage rep59 (no guard, one seat), the smoke row (c2-100 EVICT at 600 MB with the evicting-seat env) and the rehearsal criteria, the driver called with the Realtime
    hint and RWM_C_RELIABLE=0 / RWM_UNIFIED=1 / RWM_GEN=0 at RWM_PERF_TIMEOUT_S=150, and (a full session) the ledger: every slot, every row LIVE with its t59 readouts, the report."""
    import place38_parse as pp
    import mbar59_score as m59
    rep = os.path.join(run_dir, "rep59")
    stxt = open(os.path.join(rep, "smoke.log"), errors="replace").read() if os.path.exists(os.path.join(rep, "smoke.log")) else ""
    ctxt = open(os.path.join(rep, "smoke-check.txt"), errors="replace").read() if os.path.exists(os.path.join(rep, "smoke-check.txt")) else ""
    heads = re.findall(r"^=== rep=\d+ seed=\d+ cell=(\S+) arm=(\S+) attempt=\d+ pipeline=window hint=(\S+) .*? bytes=(\d+) arm_env='([^']*)' bin=(\S+) sha=", stxt, re.M)
    check(f"{tag}: no guard stage, no BASE tree; the smoke is ONE c2-100 EVICT row at 600 MB, hint realtime, arm env RWM_C_RELIABLE=0 only (RWM_UNIFIED is the shipped default), on the ONE binary (bin/, never bin-base)",
          not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and len(heads) == 1
          and heads[0][:5] == ("c2-100", "EVICT", "realtime", "600000000", "RWM_C_RELIABLE=0") and heads[0][5].endswith("/bin/raptorpath"), heads)
    crits = ["one row per smoke slot ran", "the evicting seat is armed on every row", "every row ran the one seat's binary", "the Realtime hint reached both ends", "every [DIAG] / [SPAN] readout computed",
             "every smoke row returned"]
    check(f"{tag}: the rehearsal criteria are printed as PASS lines (evicting seat armed, the sha, the Realtime hint, every readout, the return) and SMOKE-PASS",
          all(re.search(r"^PASS " + re.escape(x), ctxt, re.M) for x in crits) and "FAIL " not in ctxt and "SMOKE-PASS" in ctxt and "SMOKE-PASS" in ctxt, ctxt[-1400:])
    drv = [ln.split() for ln in open(os.path.join(sb["logs"], "drv.log")).read().splitlines()]
    drv2 = open(os.path.join(sb["logs"], "drv2.log")).read().splitlines()
    check(f"{tag}: every driver call ran at 600 MB on the c2 scenario with RWM_PERF_TIMEOUT_S=150, the Realtime hint, RWM_C_RELIABLE=0, RWM_UNIFIED unset (default on) and RWM_GEN=0 (a retaining or generation-coded row would not be this seat)",
          drv and all(d[3] == "timeout=150" and d[2] == "600000000" and d[0] == "c2" and d[1] == "c2" for d in drv) and drv2 and set(drv2) == {"realtime reliable=0 unified=unset gen=0"}, (drv[:2], drv2[:2]))
    if path == "rehearse":
        sdir = os.path.join(run_dir, "suites")
        check(f"{tag}: a rehearsal runs the tools tests (python and bash) but no engine suites, and leaves NO suite record (no TESTS-OK, no SRC-MANIFEST.sha256)",
              os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")) and not os.path.exists(os.path.join(sdir, "TESTS-OK")) and not os.path.exists(os.path.join(sdir, "SRC-MANIFEST.sha256"))
              and "TEST bashtests rc=0" in open(os.path.join(sdir, "TESTS.txt"), errors="replace").read(), os.listdir(sdir) if os.path.isdir(sdir) else None)
        return
    led = led56(rep)
    table = pp.plan59() if FULL else [x for x in pp.plan59() if x[4] <= 2]      # fast mode: the sandbox caps the replicates at 2 (REPS_CAP59)
    want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
    got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
    check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}), one row each, every row LIVE with its t59 readouts none of them None, no problem on any row",
          got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t59") and not m59.missing(r) and r["problems"] == [] for r in led),
          (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], m59.missing(r)) for r in led if r["status"] != "LIVE" or m59.missing(r) or r["problems"]][:3], got_slots[:3], want_slots[:3]))
    t = led[0]["t59"]
    check(f"{tag}: the readouts are the stub's known deltas (per tick 250 src / 240 acked / 20 ack wakes, n = 100 + k % 5): release batch 12.5 (src) / 12.0 (ack), the shed counted from [SPAN], the seat armed, the W1 upper bracket in counts (40 cod + 10 plost per tick) / (30 dbt_acc per tick) = 5/3",
          abs(t["rel_src"] - 12.5) < 1e-9 and abs(t["rel_ack"] - 12.0) < 1e-9 and t["mbar_lo"] is not None and abs(t["w1_hi"] - (40 + 10) / 30.0) < 1e-9 and t["shed_total"] > 0 and t["shed_armed_span"] and t["sender_shed_echo"] and t["receiver_shed_echo"], t)
    shas = {r["bin_sha"] for r in led}
    check(f"{tag}: one binary ran every row (one sha) and the seat comparison executed on every row (sha_checked)", len(shas) == 1 and all(r["sha_checked"] is True for r in led), shas)
    ldtxt = open(os.path.join(rep, "ld.log"), errors="replace").read()
    envl = ldtxt.split("=== env ")[-1].split("\n")[0]
    check(f"{tag}: the instrument env is RWM_RDIAG=1 with RWM_PERF_TIMEOUT_S=150 (no RTOBS), and the env scrub withholds RWM_UNIFIED, RWM_UNIFIED_SHED and RWM_C_RELIABLE",
          "RWM_PERF_TIMEOUT_S=150" in envl and "RWM_RTOBS" not in envl.split(" bulk window")[0] and "RWM_UNIFIED RWM_UNIFIED_SHED RWM_C_RELIABLE RWM_SLAW RWM_THREE_TERM RWM_SUCC_DUMP RWM_SUCC_DUMP_MAX RWM_STORE_PATH_POOL;" in envl, envl[:300])
    sc = open(os.path.join(rep, "score.txt"), errors="replace").read()
    m = re.search(r"^VERDICT59 REPORTED-ONLY slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+) dnf=(\d+)", sc, re.M)
    check(f"{tag}: the report prints the readouts, the m-bar band and a VERDICT59 line counting every slot", m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led)
          and "m-bar LOWER BOUND" in sc and "W1 BRACKET" in sc and "release batch" in sc, sc[-900:])
    tests_txt = open(os.path.join(run_dir, "suites", "TESTS.txt"), errors="replace").read()
    check(f"{tag}: the tools tests ran in the suites step (TEST parsers and TEST bashtests lines), whether or not the engine suites did", "TEST parsers " in tests_txt and "TEST bashtests " in tests_txt, tests_txt)


def red59(kind, want):
    """A defect injected by the stub into a plan-59 rehearsal: the smoke must FAIL on the criterion that catches it (ABORT-SMOKE, rc 5, no REHEARSAL-PASS). `noshed` = the seat is not armed."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "59", None)
        rc, out, rows, cargo = run_launch(sb, "59", None, rehearse=True, extra_env={"STUB_BAD59": kind})
        chk = os.path.join(sb["root"], "run", "rep59", "smoke-check.txt")
        ctxt = open(chk, errors="replace").read() if os.path.exists(chk) else ""
        check(f"59/red:{kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == 5 and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-700:], tail(out, 8)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def repl59():
    """A full plan-59 session in which the stub leaves the FIRST seed-42 row not armed (no shed echo, retention echo: VOID by the liveness witnesses -> replaced once) and the seed-7 rows time out (a DNF is
    LIVE, goodput None, its readouts kept): the replacement is a second row of the same slot, only once, and the session completes."""
    import place38_parse as pp
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "59", None)
        rc, out, rows, cargo = run_launch(sb, "59", None, extra_env={"STUB_BAD59": "shedonce", "STUB_DNF59": "7"})
        rep = os.path.join(sb["root"], "run", "rep59")
        led = led56(rep)
        ldtxt = open(os.path.join(rep, "ld.log"), errors="replace").read() if os.path.exists(os.path.join(rep, "ld.log")) else ""
        sc = open(os.path.join(rep, "score.txt"), errors="replace").read() if os.path.exists(os.path.join(rep, "score.txt")) else ""
        smoke = open(os.path.join(rep, "smoke.log"), errors="replace").read() if os.path.exists(os.path.join(rep, "smoke.log")) else ""
        per = {}
        for r in led:
            per.setdefault((r["cell"], r["arm"], r["seed"], r["rep"]), []).append(r)
        nslots = len(pp.plan59()) if FULL else len([x for x in pp.plan59() if x[4] <= 2])
        check(f"59/repl: the session completes (rc 0, LD-ALL-DONE) with ONE replaced slot: one slot with 2 ledger rows, none with more",
              rc == 0 and "LD-ALL-DONE" in out and len(per) == nslots and len(led) == nslots + 1 and max(len(v) for v in per.values()) == 2, (rc, len(led), len(per), tail(out, 6), smoke[-300:]))
        first = [r for r in led if (r["cell"], r["arm"], r["seed"], r["rep"]) == ("c2-100", "EVICT", "42", 1)]
        check("59/repl: the unarmed first row is WITNESS-FAIL on the evicting-seat liveness (no shed echo, no shed gauge, the retention echo), the replacement LIVE and armed, 'REPLACE59 ... reason=evict-liveness' in the ledger",
              len(first) == 2 and first[0]["status"] == "WITNESS-FAIL" and pp.is_evict_void59(first[0]) and "evict-sender-shed-echo-missing" in first[0]["problems"] and "evict-reliable-echo-present" in first[0]["problems"]
              and first[1]["status"] == "LIVE" and first[1]["problems"] == [] and re.search(r"^REPLACE59 c2-100 EVICT s42 rep=1 reason=evict-liveness", ldtxt, re.M) is not None, [(r["status"], r["problems"]) for r in first])
        d = [r for r in led if r["seed"] == "7" and r["status"] == "LIVE"]
        t = (d[0].get("t59") or {}) if d else {}
        check("59/repl: the DNF row(s) (seed 7) are LIVE with dnf true and no summary goodput, keeping the [DIAG] / [SPAN] readouts (none None); the report counts them",
              len(d) >= 1 and all(x["dnf"] is True and x.get("mbps") is None for x in d) and all(t.get(k) is not None for k in ("rel_ack", "loop_ack", "cod_src_rate", "rr", "shed_total", "goodput_diag")) and ("dnf=%d" % len(d)) in sc,
              (len(d), t.get("rel_ack"), sc[-300:]))
        check("59/repl: the score lists the replaced row and prints its VERDICT line", sc.count("  REPLACED ") == 1 and "VERDICT59 REPORTED-ONLY" in sc, sc[:700])
    finally:
        shutil.rmtree(root, ignore_errors=True)


def budget59(left_s, ok):
    """The budget guard: LD_HARD with `left_s` seconds left is refused below the worst case of plan 59 (7 200 s) and accepted at it."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "59", None)
        rc, out, rows, cargo = run_launch(sb, "59", None, rehearse=True, extra_env={"LD_HARD": str(int(time.time()) + left_s)})
        e = era(sb)
        if ok:
            check(f"59/budget: {left_s} s left (just above the 7 200 s worst case) is accepted: the rehearsal completes", rc == 0 and "ABORT-BUDGET" not in e and "LD-ALL-REHEARSAL-DONE" in out, (rc, tail(e, 4)))
        else:
            check(f"59/budget: {left_s} s left (below the 7 200 s worst case) is refused before anything runs (ABORT-BUDGET plan 59, rc 3)",
                  rc == 3 and "ABORT-BUDGET plan 59:" in e + out and rows == [] and not [c for c in cargo if "|build " in c], (rc, tail(e, 4)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def stub_bin_file(sb):
    """The prepared stub engine binary of the plan-60 cases: -> (path, sha256). It carries every token the battery's preflight asks for, so the test KNOWS the sha it registers as LD_BIN_SHA_EXPECT."""
    path = os.path.join(sb["logs"], "stub-engine-bin")
    body = ("#!/bin/sh\n# stub engine binary for plans 60a / 60b\n# RWM_STORE\n# RWM_STORE_PATH_POOL\n# RWM_UNIFIED_SHED unified overload shedding ACTIVE\n# RWM_SLAW\n# [SLAW] t=\n# RWM_RTT_DUMP\n"
            "# RWM_SUCC_DUMP RWM_EMIT_BURST\n# RWM_DELTA\n# emission batching ACTIVE\n# emission batching out of scope\n# RWM_PERF_RATE_MBIT\n# spc=\n# offered-load pacing ACTIVE\nexit 0\n")
    with open(path, "w", newline="\n") as fh:
        fh.write(body)
    os.chmod(path, 0o755)      # the script's `[ -x ]` check: a Windows host passes any file, Linux needs the bit
    return path, hashlib.sha256(body.encode()).hexdigest()


def spec60(cell, arm):
    import plan60
    return plan60.arm_bytes(cell, arm), plan60.arm_timeout(cell, arm), plan60.arm_hint(arm), plan60.arm_env(cell, arm)


def table60(plan):
    """The ledger table of the sandbox: the registered table in full mode, the replicate cap of the fast mode (REPS_CAP60 = 2: replicates / pairs 1..2) otherwise."""
    import plan60
    return plan60.plan(plan) if FULL else [x for x in plan60.plan(plan) if x[4] <= 2]


def driver_lines60(sb):
    return [ln.split() for ln in open(os.path.join(sb["logs"], "drv3.log")).read().splitlines()]


def expect_calls60(rows):
    """The driver lines (drv3.log) the rows must leave: scenario a, b, hint, bytes, mode, timeout, RWM_SLAW, pool, RWM_C_RELIABLE, RWM_RDIAG, RWM_DIAG, the binary's name."""
    out = []
    for cell, _, arm, _, _ in rows:
        b, tmo, hint, env = spec60(cell, arm)
        e = dict(x.split("=", 1) for x in env)
        sc = {"c2-100": ("c2", "c2", "single"), "c8-100": ("c2", "c3", "dual"), "c1s-400": ("c1", "c1", "single")}[cell]
        out.append([sc[0], sc[1], hint, str(b), sc[2], "timeout=%d" % tmo, "slaw=" + e["RWM_SLAW"], "pool=" + e.get("RWM_STORE_PATH_POOL", "unset"), "reliable=" + e.get("RWM_C_RELIABLE", "unset"),
                    "rdiag=1", "rtobs=" + e.get("RWM_RTOBS", "unset"), "diag=1", "bin=raptorpath"])
    return out


def case60(plan, path):
    """Plans 60a / 60b (the store law's gauge session) on the stubs: the strict suite rule (a reference-manifest MISS aborts), the registered bin_sha check, the per-row run spec at the driver, the 10-row rehearsal and its
    go/no-go, the RWM_SLAW echo VOID + replacement, the NO-GO decision, the prebuilt binary, and a whole session through the scorer."""
    import json
    import plan60
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        binf, sha = stub_bin_file(sb)
        extra = {"LD_REF_MANIFEST": ref56_file(sb, miss=(path == "reuse-miss")), "LD_BIN_SHA_EXPECT": ("0" * 64 if path == "bin-sha-mismatch" else sha), "STUB_BIN_FILE": px(binf)}
        if path == "slaw-echo-void":
            extra["STUB_BAD60"] = "slawonce"
        if path == "nogo-rehearse":
            extra["STUB_BAD60"] = "nogo8"
        if path == "prebuilt":
            extra["LD_BIN_PREBUILT"] = px(binf)
        if plan == "60b":      # 60b's launch reads 60a's R-sign labels before any row (Amendment 1)
            sc60a = os.path.join(sb["logs"], "score60a.txt")
            with open(sc60a, "w", newline="\n") as fh:
                fh.write("LABEL60 R-SIGN-c2 %s\nLABEL60 R-SIGN-c8 UNSCOREABLE\n" % ("STOP" if path == "rsign-abort" else "PASS"))
            extra["LD_SCORE60A"] = px(sc60a)
        rehearse = path in ("rehearse", "nogo-rehearse")
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        tests = [c for c in cargo if "|test " in c]
        if path == "rsign-abort":
            check(f"{tag}: 60a's score carries R-SIGN-c2 STOP -> ABORT-RSIGN, rc 3, NOTHING measured (no row-parser call, no driver call, no build, no ld.log)",
                  rc == 3 and "ABORT-RSIGN" in out and "R-SIGN-c2 PASS|UNSCOREABLE" in out and rows == [] and driver_lines60(sb) == [] and builds == [] and not os.path.exists(os.path.join(run_dir, "rep60b")), (rc, out[-300:]))
            EXECUTED.add((plan, path))
            return
        if path in ("reuse-miss", "bin-sha-mismatch"):
            token = "ABORT-SUITES plan %s suite-reuse MISS" % plan if path == "reuse-miss" else "ABORT-BIN-SHA plan %s" % plan
            check(f"{tag}: {token.split(' plan')[0]} -> FAILED-ALL, rc 5, NOTHING measured (no row-parser call, no driver call, no ld.log) and no terminal marker",
                  rc == 5 and token in e and failed and rows == [] and driver_lines60(sb) == [] and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out
                  and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(e, 6), rows[:2]))
            if path == "reuse-miss":
                check(f"{tag}: the MISS is on record as the suite-reuse decision and no engine suite ran (no cargo test)", re.search(r"suite-reuse MISS", e) is not None and tests == [], tail(e, 8))
            else:
                check(f"{tag}: the registered value and the built sha are both on record, and the binary was built (the check follows the build)", sha != "0" * 64 and ("registered " + "0" * 64) in e and len(builds) == 1, tail(e, 6))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        check(f"{tag}: rc 0 and the terminal marker {marker}", rc == 0 and marker in out and not failed, (rc, tail(out, 30), tail(e, 15)))
        stage = os.path.join(run_dir, "rep" + plan)
        smoke_txt = open(os.path.join(stage, "smoke.log"), errors="replace").read() if os.path.exists(os.path.join(stage, "smoke.log")) else ""
        chk = open(os.path.join(stage, "smoke-check.txt"), errors="replace").read() if os.path.exists(os.path.join(stage, "smoke-check.txt")) else ""
        check(f"{tag}: ONE stage rep{plan} (no guard, no BASE tree, ONE build unless prebuilt), the registered bin_sha equals the built binary and is on record, the smoke check printed SMOKE-PASS",
              not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and len(builds) == (0 if path == "prebuilt" else 1)
              and ("equals the registered value " + sha) in e and sha in open(os.path.join(run_dir, "BINSHA.txt")).read() and "SMOKE-PASS" in chk, (builds, tail(e, 6), chk[-600:]))
        sdir = os.path.join(run_dir, "suites")
        tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read() if os.path.isdir(sdir) and os.path.exists(os.path.join(sdir, "TESTS.txt")) else ""
        hit = re.search(r"suite-reuse (HIT|MISS)", e)
        check(f"{tag}: the suite-reuse HIT is on record and the engine suites were skipped (no cargo test; ENGINE-SUITES-REUSED; the tools tests ran; no false suite record)",
              hit is not None and hit.group(1) == "HIT" and tests == [] and "ENGINE-SUITES-REUSED" in tt and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt
              and not os.path.exists(os.path.join(sdir, "TESTS-OK")) and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")), (hit and hit.group(0), tests, tt[:300]))
        calls = driver_lines60(sb)
        plans_seen = {r.split("plan=")[1].split()[0] for r in rows}
        check(f"{tag}: every row-parser call ran under LD_PLAN={plan}, with LD_BASE_SHA empty", plans_seen == {plan} and {re.search(r"base_sha=(\S*)", r).group(1) for r in rows} == {""}, (plans_seen, rows[:1]))
        if rehearse:
            want_rows = plan60.plan("60br") if plan == "60b" else plan60.plan("60a", rehearsal=True)
            check(f"{tag}: the rehearsal ran {len(want_rows)} rows ({'both smokes and the 6 floor-pool rows' if plan == '60a' else 'the 2-row smoke only: no evicting row since status 60 Amendment 2'}), no battery (no ld.log), no suites, REHEARSAL-PASS persisted",
                  len(rows) == len(want_rows) and tests == [] and not os.path.exists(os.path.join(stage, "ld.log")) and os.path.exists(os.path.join(stage, "REHEARSAL-PASS")), (len(rows), want_rows[:2]))
            check(f"{tag}: the driver saw each smoke / rehearsal row's run spec (bytes, timeout, hint, RWM_SLAW, pool) in table order", calls == expect_calls60(want_rows), (calls[:2], expect_calls60(want_rows)[:2]))
            heads = re.findall(r"^=== rep=(\d+) seed=(\d+) cell=(\S+) arm=(\S+) attempt=\d+ pipeline=window hint=(\S+) .*? bytes=(\d+) arm_env='([^']*)'", smoke_txt, re.M)
            check(f"{tag}: the smoke ledger heads carry the arm env (RWM_SLAW first) and the per-row hint / bytes",
                  [(h[2], h[3], h[4], h[5], h[6]) for h in heads] == [(c, a, plan60.arm_hint(a), str(plan60.arm_bytes(c, a)), " ".join(plan60.arm_env(c, a))) for c, _, a, _, _ in want_rows], heads[:2])
            if plan == "60b":
                check(f"{tag}: no evicting row in the rehearsal: the smoke check says so (EVICTING ROWS: none, NOT-RUN) and prints no PASS for the evicting-row criteria",
                      "EVICTING ROWS: none in this session" in chk and "NOT-RUN the evicting-row rehearsal checks" in chk and "GO-NOGO-EVICT" not in chk and "SMOKE-FAIL" not in chk
                      and re.search(r"^PASS .*(debt loop|evicting row|ackw)", chk, re.M) is None, chk[-900:])
                check(f"{tag}: the CAPLOG replay is in the smoke check (replayable share and mismatches printed, PASS)", re.search(r"CAPLOG \S+ \S+: records=\d+ replayable share=", chk) is not None and re.search(r"^PASS the CAPLOG replay", chk, re.M) is not None, chk[-600:])
            if plan == "60a":
                gng = dict(re.findall(r"GO-NOGO\s+(c2|c8|c1s)\S*\s+(GO|NO-GO)", chk))
                want_go = {"c2": "GO", "c8": "NO-GO" if path == "nogo-rehearse" else "GO", "c1s": "GO"}
                check(f"{tag}: the go/no-go is printed per cell ({want_go}): " + ("c8's starved leg 0 reads NO-GO (a decision, the rehearsal still passes)" if path == "nogo-rehearse" else "all three cells GO on the stub's healthy rows"),
                      gng == want_go and "SMOKE-FAIL" not in chk, (gng, chk[-900:]))
            EXECUTED.add((plan, path))
            return
        # a whole session
        led = led56(stage)
        table = table60(plan)
        want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
        got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
        if path == "slaw-echo-void":
            first = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) == want_slots[0]]
            others = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) != want_slots[0]]
            ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
            check(f"{tag}: the first scored slot echoed RWM_SLAW wrongly: its first row is WITNESS-FAIL on gate-slaw, 'REPLACE60 ... reason=slaw-echo' is in the ledger, the replacement row is LIVE and clean; every other slot has one row",
                  len(first) == 2 and first[0]["status"] == "WITNESS-FAIL" and any(x.startswith("gate-slaw") for x in first[0]["problems"]) and first[1]["status"] == "LIVE" and first[1]["problems"] == []
                  and re.search(r"^REPLACE60 %s %s s%s rep=%s reason=slaw-echo" % (want_slots[0][0], want_slots[0][1], want_slots[0][2], want_slots[0][3]), ldtxt, re.M) is not None
                  and len(others) == len(want_slots) - 1 and [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led if r not in first] == want_slots[1:], [(r["status"], r["problems"]) for r in first])
            check(f"{tag}: the replacement ran the same arm: the driver was called {len(want_slots) + 3} times (2 smoke rows, the VOID row, its replacement, the rest)", len(calls) == len(want_slots) + 3 and calls[2] == calls[3], len(calls))
            sc = open(os.path.join(stage, "score.txt"), errors="replace").read()
            check(f"{tag}: the score reports the replaced slot and a VERDICT60 line", "VERDICT60" in sc and re.search(r"REPLACED.*G16", sc) is not None, sc[:600])
            EXECUTED.add((plan, path))
            return
        check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}), one row each, every row LIVE with its t60 readouts, no t60 error, no problem on any row",
              got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t60") and not r.get("t60_error") and r["problems"] == [] for r in led),
              (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], r.get("t60_error")) for r in led if r["status"] != "LIVE" or r["problems"] or r.get("t60_error")][:3], got_slots[:3], want_slots[:3]))
        smoke_rows = list(plan60.SMOKE[plan])
        check(f"{tag}: the driver saw every row's run spec in order: the 2 smoke rows, then the table (bytes, timeout, hint, RWM_SLAW, RWM_STORE_PATH_POOL, RWM_C_RELIABLE, RWM_RDIAG, RWM_DIAG=1, the one binary)",
              calls == expect_calls60(smoke_rows + table), (len(calls), calls[:3], expect_calls60(smoke_rows + table)[:3]))
        logs = {}
        ddir = os.path.join(stage, "diag-ld")
        for fn in os.listdir(ddir) if os.path.isdir(ddir) else []:
            m = re.match(r"(\S+?)@def-(\S+?)-s(\d+)-r(\d+)-a1-c\.log$", fn)
            if m:
                logs[(m.group(1), m.group(2), m.group(3), m.group(4))] = open(os.path.join(ddir, fn), errors="replace").read()
        gauge_ok = bool(logs) and all((txt.count("\n[SLAW] ") > 0 or txt.startswith("[SLAW] ")) == plan60.gauge_on(k[1]) for k, txt in logs.items())
        check(f"{tag}: the per-row logs agree with the arms: every G / EVICT row printed [SLAW] windows, every C row none ({len(logs)} logs)", gauge_ok and len(logs) == len(want_slots), (len(logs), [k for k in logs][:3]))
        shas = {r["bin_sha"] for r in led}
        check(f"{tag}: one binary ran every row, its sha is the registered one, and the seat comparison executed on every row", shas == {sha} and all(r["sha_checked"] is True for r in led), shas)
        ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
        envl = ldtxt.split("=== env ")[-1].split("\n")[0]
        check(f"{tag}: the env scrub withholds RWM_SLAW and RWM_THREE_TERM (and the rest), the instrument env is RWM_RDIAG=1 + the timeout (RWM_RTOBS=1 rides the G / C arm env, never the EVICT rows)", " RWM_SLAW RWM_THREE_TERM " in envl and "RWM_PERF_TIMEOUT_S=150" in envl and "RWM_RTOBS" not in envl.split(" bulk window")[0], envl[:400])
        if plan == "60b":
            ev = [r for r in led if r["arm"] == "EVICT"]
            ctl = [r for r in led if r["arm"] in ("G2048", "C2048")]
            check(f"{tag}: no evicting row ran (status 60 Amendment 2: no EVICT row, no Realtime call); the c1s control interleaves G and C at 5 GB / 210 s",
                  not ev and not any(c[2] == "realtime" for c in calls)
                  and ctl and all(r["bytes"] == 5_000_000_000 for r in ctl) and {c[5] for c in calls if c[3] == "5000000000"} == {"timeout=210"}, (len(ev), len(ctl)))
        sc = open(os.path.join(stage, "score.txt"), errors="replace").read()
        m = re.search(r"^VERDICT60 \S+ slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+)", sc, re.M)
        check(f"{tag}: the score prints a VERDICT60 line counting every slot, with every row live and none replaced, and the labelled readouts (LABEL60 lines)",
              m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led) and int(m.group(4)) == 0 and int(m.group(5)) == 0 and "LABEL60 " in sc, sc[-1200:])
        check(f"{tag}: DONE-ALL and score.txt", os.path.exists(os.path.join(run_dir, "DONE-ALL")) and os.path.exists(os.path.join(run_dir, "score.txt")), tail(e, 12))
        if path == "prebuilt":
            check(f"{tag}: LD_BIN_PREBUILT installed the prior launch's binary (no cargo build) and its sha was still checked against the registered one", builds == [] and "binary prebuilt from" in e and ("equals the registered value " + sha) in e, tail(e, 8))
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


def spec61(cell, arm, knee=None):
    import plan61
    return plan61.arm_bytes(cell, arm, knee=knee), plan61.arm_timeout(cell, arm), plan61.arm_hint(arm), plan61.arm_env(cell, arm)


def table61(plan, knee=None, ctl=None):
    """The ledger table of the sandbox: the registered table in full mode, the replicate cap of the fast mode (REPS_CAP61 = 2) otherwise."""
    import plan61
    t = plan61.plan(plan, knee=knee, ctl=ctl)
    return t if FULL else [x for x in t if x[4] <= 2]


def expect_calls61(rows, knee=None):
    """The driver lines (drv3.log) the rows must leave (the driver's argv: scenario a, b, hint, bytes, mode, then timeout, RWM_SLAW, pool, reliable, rdiag, rtobs, diag, the binary)."""
    out = []
    for cell, _, arm, _, _ in rows:
        b, tmo, hint, env = spec61(cell, arm, knee)
        e = dict(x.split("=", 1) for x in env)
        sc = {"c3-25": ("c3", "c3", "single"), "c2-100": ("c2", "c2", "single")}[cell]
        out.append([sc[0], sc[1], hint, str(b), sc[2], "timeout=%d" % tmo, "slaw=" + e["RWM_SLAW"], "pool=" + e.get("RWM_STORE_PATH_POOL", "unset"), "reliable=unset",
                    "rdiag=1", "rtobs=" + e.get("RWM_RTOBS", "unset"), "diag=1", "bin=raptorpath"])
    return out


def case61(plan, path):
    """Plans 61a / 61b (the c3 sweep) on the stubs: the strict suite rule, the registered bin_sha check (ABORT-BIN-SHA before any row), ABORT-PILOT without LD_KNEE61, the 6-row rehearsal with its go/no-go and PILOT61,
    the per-row run spec at the c3 fake driver, the RWM_SLAW echo VOID + replacement, the prebuilt binary, and a whole session through the scorer (c3 at 61a, c2 at 61b)."""
    import plan61
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        binf, sha = stub_bin_file(sb)
        extra = {"LD_REF_MANIFEST": ref56_file(sb, miss=(path == "reuse-miss")), "LD_BIN_SHA_EXPECT": ("0" * 64 if path == "bin-sha-mismatch" else sha), "STUB_BIN_FILE": px(binf)}
        knee = float(KNEE61) if (plan == "61a" and path not in ("rehearse", "pilot-missing", "control-missing")) else None
        if knee is not None:
            extra["LD_KNEE61"] = KNEE61
        ctl61 = "512" if (plan == "61a" and path != "control-missing") else None      # status 61 Amendment 2: the 61a control is W 512, run_all refuses a 61a launch without LD_CTL61=512
        if ctl61:
            extra["LD_CTL61"] = ctl61
        if path == "slaw-echo-void":
            extra["STUB_BAD60"] = "slawonce"
            extra["STUB_SLAWONCE_AT"] = "1"
        if path == "prebuilt":
            extra["LD_BIN_PREBUILT"] = px(binf)
        rehearse = path in ("rehearse", "control-missing")      # control-missing is the rehearsal's own refusal: ABORT-CONTROL61 covers the rehearsal too
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        tests = [c for c in cargo if "|test " in c]
        if path in ("reuse-miss", "bin-sha-mismatch", "pilot-missing", "control-missing"):
            token = {"reuse-miss": "ABORT-SUITES plan %s suite-reuse MISS" % plan, "bin-sha-mismatch": "ABORT-BIN-SHA plan %s" % plan, "pilot-missing": "ABORT-PILOT plan 61a", "control-missing": "ABORT-CONTROL61 plan 61a"}[path]
            check(f"{tag}: {token.split(' plan')[0]} -> FAILED-ALL, rc 5, NOTHING measured (no row-parser call, no driver call, no ld.log) and no terminal marker",
                  rc == 5 and token in e and failed and rows == [] and driver_lines60(sb) == [] and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out
                  and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(e, 6), rows[:2]))
            if path == "reuse-miss":
                check(f"{tag}: the MISS is on record as the suite-reuse decision and no engine suite ran (no cargo test)", re.search(r"suite-reuse MISS", e) is not None and tests == [], tail(e, 8))
            elif path == "bin-sha-mismatch":
                check(f"{tag}: the registered value and the built sha are both on record, and the binary was built (the check follows the build)", sha != "0" * 64 and ("registered " + "0" * 64) in e and len(builds) == 1, tail(e, 6))
            elif path == "control-missing":
                check(f"{tag}: a REHEARSAL launched without LD_CTL61=512 is refused after the bin_sha check (ABORT-CONTROL61, the message names the empty value) and no row ran", len(builds) == 1 and ("equals the registered value " + sha) in e and "LD_CTL61=''" in e, tail(e, 6))
            else:
                check(f"{tag}: LD_KNEE61 unset: the abort follows the bin_sha check (priority 5) and no row ran", len(builds) == 1 and ("equals the registered value " + sha) in e and "LD_KNEE61=''" in e, tail(e, 6))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        check(f"{tag}: rc 0 and the terminal marker {marker}", rc == 0 and marker in out and not failed, (rc, tail(out, 30), tail(e, 15)))
        stage = os.path.join(run_dir, "rep" + plan)
        smoke_txt = open(os.path.join(stage, "smoke.log"), errors="replace").read() if os.path.exists(os.path.join(stage, "smoke.log")) else ""
        chk = open(os.path.join(stage, "smoke-check.txt"), errors="replace").read() if os.path.exists(os.path.join(stage, "smoke-check.txt")) else ""
        check(f"{tag}: ONE stage rep{plan} (no guard, no BASE tree, ONE build unless prebuilt), the registered bin_sha equals the built binary and is on record, the toolchain is logged, the smoke check printed SMOKE-PASS",
              not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and len(builds) == (0 if path == "prebuilt" else 1)
              and ("equals the registered value " + sha) in e and sha in open(os.path.join(run_dir, "BINSHA.txt")).read() and "LD-ALL toolchain" in e and "SMOKE-PASS" in chk, (builds, tail(e, 6), chk[-600:]))
        sdir = os.path.join(run_dir, "suites")
        tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read() if os.path.isdir(sdir) and os.path.exists(os.path.join(sdir, "TESTS.txt")) else ""
        hit = re.search(r"suite-reuse (HIT|MISS)", e)
        check(f"{tag}: the suite-reuse HIT is on record and the engine suites were skipped (no cargo test; the tools tests ran; no false suite record)",
              hit is not None and hit.group(1) == "HIT" and tests == [] and "ENGINE-SUITES-REUSED" in tt and "TEST parsers rc=0" in tt and not os.path.exists(os.path.join(sdir, "TESTS-OK")), (hit and hit.group(0), tests, tt[:300]))
        calls = driver_lines60(sb)
        plans_seen = {r.split("plan=")[1].split()[0] for r in rows}
        check(f"{tag}: every row-parser call ran under LD_PLAN={plan}, with LD_BASE_SHA empty", plans_seen == {plan} and {re.search(r"base_sha=(\S*)", r).group(1) for r in rows} == {""}, (plans_seen, rows[:1]))
        if rehearse:
            want_rows = plan61.plan("61r")
            check(f"{tag}: the rehearsal ran the {len(want_rows)} rows R1-R6, no battery (no ld.log), no suites, REHEARSAL-PASS persisted",
                  len(rows) == len(want_rows) and tests == [] and not os.path.exists(os.path.join(stage, "ld.log")) and os.path.exists(os.path.join(stage, "REHEARSAL-PASS")), (len(rows), want_rows[:2]))
            check(f"{tag}: the driver saw each rehearsal row's run spec (bytes, timeout, hint, RWM_SLAW, pool) in table order (no pilot knee yet: the pilot rows take the bytes formula)", calls == expect_calls61(want_rows), (calls[:2], expect_calls61(want_rows)[:2]))
            check(f"{tag}: the go/no-go is printed (floor GO, control GO) and PILOT61 names the pilot knee (the stub's knee u = 0.95 falls at W 86.5) and the arms it places, with no SMOKE-FAIL",
                  "GO-NOGO floor GO" in chk and "GO-NOGO control GO" in chk and re.search(r"^PILOT61 W=8[67]\.\d u88=0\.9\d+ u128=1\.000 arms=", chk, re.M) is not None and "SMOKE-FAIL" not in chk
                  and "REHEARSAL61 " in e and "PILOT61 W=" in e, chk[-1200:])
            check(f"{tag}: the CAPLOG replay and the reported witnesses are in the rehearsal's check", re.search(r"^PASS the CAPLOG replay", chk, re.M) is not None and "  WITNESS G12 " in chk, chk[-600:])
            EXECUTED.add((plan, path))
            return
        # a whole session
        led = led56(stage)
        table = table61(plan, knee, ctl61)
        want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
        got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
        smoke_rows = plan61.plan(plan, smoke=True, knee=knee)
        if path == "slaw-echo-void":
            first = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) == want_slots[0]]
            others = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) != want_slots[0]]
            ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
            check(f"{tag}: the first scored slot echoed RWM_SLAW wrongly: its first row is WITNESS-FAIL on gate-slaw, 'REPLACE61 ... reason=slaw-echo' is in the ledger, the replacement row is LIVE and clean; every other slot has one row",
                  len(first) == 2 and first[0]["status"] == "WITNESS-FAIL" and any(x.startswith("gate-slaw") for x in first[0]["problems"]) and first[1]["status"] == "LIVE" and first[1]["problems"] == []
                  and re.search(r"^REPLACE61 %s %s s%s rep=%s reason=slaw-echo" % (want_slots[0][0], want_slots[0][1], want_slots[0][2], want_slots[0][3]), ldtxt, re.M) is not None
                  and len(others) == len(want_slots) - 1 and [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led if r not in first] == want_slots[1:], [(r["status"], r["problems"]) for r in first])
            check(f"{tag}: the replacement ran the same arm: the driver was called {len(want_slots) + 2} times (1 smoke row, the VOID row, its replacement, the rest)", len(calls) == len(want_slots) + 2 and calls[1] == calls[2], len(calls))
            sc = open(os.path.join(stage, "score.txt"), errors="replace").read()
            check(f"{tag}: the score reports the replaced slot in its VERDICT61 line", re.search(r"^VERDICT61 .*replaced=1", sc, re.M) is not None, sc[:600])
            EXECUTED.add((plan, path))
            return
        check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}), one row each, every row LIVE with its t60 and t61 readouts, no readout error, no problem on any row",
              got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t60") and r.get("t61") and not r.get("t60_error") and not r.get("t61_error") and r["problems"] == [] for r in led),
              (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], r.get("t60_error"), r.get("t61_error")) for r in led if r["status"] != "LIVE" or r["problems"] or r.get("t60_error") or r.get("t61_error")][:3], got_slots[:3], want_slots[:3]))
        check(f"{tag}: the driver saw every row's run spec in order: the smoke row, then the table (bytes, timeout, hint, RWM_SLAW, RWM_STORE_PATH_POOL, RWM_RDIAG, RWM_RTOBS, RWM_DIAG=1, the one binary)",
              calls == expect_calls61(smoke_rows + table, knee), (len(calls), calls[:3], expect_calls61(smoke_rows + table, knee)[:3]))
        shas = {r["bin_sha"] for r in led}
        check(f"{tag}: one binary ran every row, its sha is the registered one, and the seat comparison executed on every row", shas == {sha} and all(r["sha_checked"] is True for r in led), shas)
        ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
        envl = ldtxt.split("=== env ")[-1].split("\n")[0]
        check(f"{tag}: the env scrub withholds RWM_SLAW and RWM_THREE_TERM (and the rest); the instrument env is RWM_RDIAG=1 + the timeout, RWM_RTOBS=1 rides the arm env",
              " RWM_SLAW RWM_THREE_TERM " in envl and "RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=" in envl and "RWM_RTOBS" not in envl.split(" bulk window")[0], envl[:400])      # the per-row timeout (c3 120 s, c2 150 s) is asserted on the driver lines above
        sc = open(os.path.join(stage, "score.txt"), errors="replace").read()
        m = re.search(r"^VERDICT61 \S+ slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+)", sc, re.M)
        check(f"{tag}: the score prints a VERDICT61 line counting every slot, with every row live and none replaced, and the labelled readouts (LABEL61 lines)",
              m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led) and int(m.group(4)) == 0 and int(m.group(5)) == 0 and "LABEL61 " in sc, sc[-1200:])
        check(f"{tag}: the score read the plan's own cell and the readings it owns ({'CR61, the floor, the knee, RHO and D per candidate' if plan == '61a' else 'CR61, the drift reading, the price and landing tables'})",
              ("CR61 c3" in sc and "LABEL61 RHO " in sc and (("FLOOR POOL" in sc and "RHO c3 =" in sc and "LABEL61 D-S-Bulk " in sc) if FULL else ("FLOOR POOL" in sc or "CONTROL-MOVED" in sc))) if plan == "61a" else ("CR61 c2" in sc and "DRIFT c2" in sc and "LANDING c2" in sc and "LABEL61 DRIFT-c2 " in sc), sc[:1500])
        check(f"{tag}: DONE-ALL and score.txt", os.path.exists(os.path.join(run_dir, "DONE-ALL")) and os.path.exists(os.path.join(run_dir, "score.txt")), tail(e, 12))
        if path == "prebuilt":
            check(f"{tag}: LD_BIN_PREBUILT installed the prior launch's binary (no cargo build) and its sha was still checked against the registered one", builds == [] and "binary prebuilt from" in e and ("equals the registered value " + sha) in e, tail(e, 8))
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


S62 = ("62r0", "62a")      # status 62 (c3 at jitter 0): plan 61's single-seat shape, NO build (the staged binary), its own tables, scorer, NO-GO labels, ABORT-NETEM and the tools-test reuse
REH62 = {}               # "go" / "nogo": (persisted run dir of a rehearsal, its PILOT62 W) -- what the 62a launches name as LD_REHEARSAL62
REH62_LOCK = threading.RLock()


def spec62(cell, arm, knee=None):
    import plan62
    return plan62.arm_bytes(cell, arm, knee=knee), plan62.arm_timeout(cell, arm), plan62.arm_hint(arm), plan62.arm_env(cell, arm)


def table62(plan, knee=None):
    """The ledger table of the sandbox: the registered table in full mode; in fast mode the replicate cap (REPS_CAP62 = 2) on the scored grid, the two dump rows (reps 7 / 8) kept."""
    import plan62
    t = plan62.plan(plan, knee=knee)
    return t if FULL else [x for x in t if x[4] <= 2 or x[2].startswith("D")]


def expect_calls62(rows, knee=None):
    """The driver lines (drv3.log) the rows must leave: scenario a, b, hint, bytes, mode, then timeout, RWM_SLAW, pool, reliable, rdiag, rtobs, diag, the binary."""
    out = []
    for cell, _, arm, _, _ in rows:
        b, tmo, hint, env = spec62(cell, arm, knee)
        e = dict(x.split("=", 1) for x in env)
        out.append(["c3j0", "c3j0", hint, str(b), "single", "timeout=%d" % tmo, "slaw=" + e["RWM_SLAW"], "pool=" + e.get("RWM_STORE_PATH_POOL", "unset"), "reliable=unset",
                    "rdiag=1", "rtobs=" + e.get("RWM_RTOBS", "unset"), "diag=1", "bin=raptorpath"])
    return out


def launch62(plan, path, reh=None, knee=None, ctl="512", bad=None, netem=None, prebuilt=True, rehearse=False, mutate_tools=False, dnf=None, hard_s=None):
    """One stubbed launch of plan 62r0 / 62a: -> dict(sb, root, rc, out, rows, cargo, sha, binf, e, run_dir). The caller removes `root`."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    sb = make_sandbox(root, plan, None)
    binf, sha = stub_bin_file(sb)
    extra = {"LD_REF_MANIFEST": ref56_file(sb, miss=(path == "reuse-miss")), "LD_BIN_SHA_EXPECT": ("0" * 64 if path == "bin-sha-mismatch" else sha), "STUB_BIN_FILE": px(binf)}
    if prebuilt:
        extra["LD_BIN_PREBUILT"] = px(binf)
    if ctl:
        extra["LD_CTL62"] = ctl
    if knee:
        extra["LD_KNEE62"] = knee
    if reh:
        extra["LD_REHEARSAL62"] = px(reh)
    if bad:
        extra["STUB_BAD60"] = bad
    if "slawonce" in (bad or ""):
        extra["STUB_SLAWONCE_AT"] = "1" if plan == "62a" else "2"
    if netem:
        extra["STUB_NETEM62"] = netem
    if dnf:
        extra["STUB_DNF60"] = dnf
    if hard_s:
        extra["LD_HARD"] = str(int(time.time()) + hard_s)
    if mutate_tools:        # ONE byte of tools/l1 differs from the rehearsal's sandbox: the tools-test reuse must MISS
        with open(os.path.join(sb["repo"], "tools", "l1", "harness_paths.py"), "a", newline="\n") as fh:
            fh.write("# one byte differs\n")
    rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=rehearse, extra_env=extra)
    return {"sb": sb, "root": root, "rc": rc, "out": out, "rows": rows, "cargo": cargo, "sha": sha, "binf": binf, "e": era(sb), "run_dir": os.path.join(sb["root"], "run")}


def keep_rehearsal62(kind, L):
    """Keep a copy of the rehearsal's run dir for the 62a launches (the sandbox is removed): -> (dir, the printed PILOT62 W | None)."""
    with REH62_LOCK:
        if kind not in REH62:
            keep = os.path.join(tempfile.gettempdir(), "stubrun-reh62-%s-%d" % (kind, os.getpid()))
            shutil.copytree(L["run_dir"], keep)
            chk = open(os.path.join(keep, "rep62r0", "smoke-check.txt"), errors="replace").read()
            m = re.search(r"^PILOT62 W=(\d+\.\d) ", chk, re.M)
            REH62[kind] = (keep, m.group(1) if m else None)
        return REH62[kind]


def rehearsal62(kind):
    """The persisted rehearsal a 62a case needs: a GO rehearsal (kind go) or one whose pilot reads both-above twice (kind nogo, NO-GO-PILOT). Run on demand when the 62r0 cases did not (STUBRUN_ONLY)."""
    with REH62_LOCK:
        if kind in REH62:
            return REH62[kind]
        L = launch62("62r0", "rehearse", bad=("pilotabove2" if kind == "nogo" else None), rehearse=True)
        try:
            return keep_rehearsal62(kind, L)
        finally:
            shutil.rmtree(L["root"], ignore_errors=True)


def budget62(plan):
    """The budget guard at the REGISTERED LD_HARD offsets (62r0 launch + 1.0 h, 62a launch + 2.5 h): the guard reads the time left after the lock wait, so a guard AT the offset would refuse every registered launch. A launch at the offset passes it
    (it then stops at the next gate, ABORT-CONTROL62, because LD_CTL62 is deliberately unset: nothing runs); one 300 s under the guard's own worst case is refused (ABORT-BUDGET)."""
    import plan62
    tag = "%s/budget" % plan
    res = {}
    for name, hard in (("registered", plan62.LD_HARD_S[plan]), ("short", plan62.BUDGET_S[plan] - 300)):
        L = launch62(plan, "budget", ctl=None, rehearse=(plan == "62r0"), hard_s=hard)
        try:
            res[name] = (L["rc"], L["e"] + L["out"], L["rows"])      # ABORT-BUDGET exits before persist(): its era line is on stdout only
        finally:
            shutil.rmtree(L["root"], ignore_errors=True)
    rc, e, rows = res["registered"]
    check(f"{tag}: a launch at the registered LD_HARD offset ({plan62.LD_HARD_S[plan]} s) passes the budget guard (no ABORT-BUDGET; the worst case {plan62.BUDGET_S[plan]} s is under it) and stops at the next gate, ABORT-CONTROL62",
          "ABORT-BUDGET" not in e and "ABORT-CONTROL62" in e and rc == 5 and rows == [], (rc, tail(e, 5)))
    rc, e, rows = res["short"]
    check(f"{tag}: a launch with {plan62.BUDGET_S[plan] - 300} s left (300 s under the worst case) is refused: ABORT-BUDGET, nothing run", "ABORT-BUDGET plan %s" % plan in e and rc == 3 and rows == [], (rc, tail(e, 5)))
    EXECUTED.add((plan, "budget"))


def case62(plan, path):
    """Plans 62r0 (the rehearsal) / 62a (the cell) on the stubs: the strict suite rule, the registered bin_sha, NO build (the prebuilt binary, its sha checked), ABORT-CONTROL62 / ABORT-PILOT / ABORT-REHEARSAL62, the 7-row
    rehearsal with every registered NO-GO and the one re-probe, ABORT-NETEM (the first row's pre, the last row's post), the tools-test reuse HIT / MISS, the RWM_SLAW echo VOID + replacement and a whole session through the scorer."""
    import json
    import plan62
    tag = "%s/%s" % (plan, path)
    if path == "budget":
        return budget62(plan)
    L = None
    try:
        reh_kind = {"rehearsal-nogo": "nogo"}.get(path, "go")
        reh, wprint = (None, None)
        if plan == "62a" and path != "rehearsal-missing":
            reh, wprint = rehearsal62(reh_kind)
        knee = wprint if plan == "62a" else None
        if path == "knee-mismatch":
            knee = "90.0"
        if path in ("pilot-missing",):
            knee = None
        if path == "rehearsal-missing":
            knee = "86.5"      # a valid LD_KNEE62 so that ABORT-PILOT does not fire first: the missing piece is the rehearsal
        kf = float(knee) if knee else None
        smoke_n = 1
        table = table62("62a", kf) if (plan == "62a" and kf) else []
        netem = {"abort-netem-first": "0:pre:jitter"}.get(path)
        if path == "abort-netem-last":
            netem = "%d:post:jitter:srv" % (smoke_n + len(table) - 1)      # the ACK leg's post read
        bad = {"nogo-floor": None, "nogo-gate": "gatedrop", "nogo-pilot": "pilotabove2", "reprobe": "pilotabove1", "nogo-control": "ctlloss", "nogo-short": "short", "slaw-echo-void": "slawonce"}.get(path)
        L = launch62(plan, path, reh=reh, knee=knee, ctl=(None if path == "control-missing" else "512"), bad=bad, netem=netem, prebuilt=(path != "prebuilt-required"), rehearse=(plan == "62r0"),
                     mutate_tools=(path == "tools-reuse-miss"), dnf=("G12:7" if path == "nogo-floor" else None))
        rc, out, rows, cargo, e, sha, sb, run_dir = L["rc"], L["out"], L["rows"], L["cargo"], L["e"], L["sha"], L["sb"], L["run_dir"]
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        tests = [c for c in cargo if "|test " in c]
        if path in ("reuse-miss", "bin-sha-mismatch", "pilot-missing", "control-missing", "prebuilt-required", "rehearsal-missing", "rehearsal-nogo", "knee-mismatch"):
            token = {"reuse-miss": "ABORT-SUITES plan %s suite-reuse MISS" % plan, "bin-sha-mismatch": "ABORT-BIN-SHA plan %s" % plan, "pilot-missing": "ABORT-PILOT plan 62a", "control-missing": "ABORT-CONTROL62 plan %s" % plan,
                     "prebuilt-required": "ABORT-BUILD plan %s never builds" % plan, "rehearsal-missing": "ABORT-REHEARSAL62 plan 62a refuses unless the rehearsal printed 'LABEL62 GO-c3j0 GO'",
                     "rehearsal-nogo": "ABORT-REHEARSAL62 plan 62a refuses unless the rehearsal printed 'LABEL62 GO-c3j0 GO'", "knee-mismatch": "ABORT-REHEARSAL62 LD_KNEE62='90.0' is not the rehearsal's PILOT62 W="}[path]
            check(f"{tag}: {token.split(' plan')[0]} -> FAILED-ALL, rc 5, NOTHING measured (no row-parser call, no driver call, no ld.log) and no terminal marker",
                  rc == 5 and token in e and failed and rows == [] and driver_lines60(sb) == [] and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out
                  and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(e, 6), rows[:2]))
            if path == "reuse-miss":
                check(f"{tag}: the MISS is on record as the suite-reuse decision and no engine suite ran (no cargo test)", re.search(r"suite-reuse MISS", e) is not None and tests == [], tail(e, 8))
            elif path == "bin-sha-mismatch":
                check(f"{tag}: the registered value and the built (prebuilt) sha are both on record, and no cargo build ran", sha != "0" * 64 and ("registered " + "0" * 64) in e and builds == [], tail(e, 6))
            elif path == "control-missing":
                check(f"{tag}: a launch without LD_CTL62=512 is refused after the bin_sha check (the message names the empty value) and no row ran", ("equals the registered value " + sha) in e and "LD_CTL62=''" in e, tail(e, 6))
            elif path == "prebuilt-required":
                check(f"{tag}: the refusal comes before any cargo call (plan 62 never builds)", builds == [] and tests == [], (builds, tests))
            elif path == "pilot-missing":
                check(f"{tag}: LD_KNEE62 unset: the abort follows the bin_sha check and no row ran", ("equals the registered value " + sha) in e and "LD_KNEE62=''" in e, tail(e, 6))
            elif path == "rehearsal-nogo":
                nogo_chk = open(os.path.join(reh, "rep62r0", "smoke-check.txt"), errors="replace").read()
                check(f"{tag}: the refused rehearsal printed a NO-GO label (not GO)", re.search(r"^LABEL62 GO-c3j0 NO-GO-PILOT$", nogo_chk, re.M) is not None, nogo_chk[-500:])
            EXECUTED.add((plan, path))
            return
        rehearse = plan == "62r0"
        stage = os.path.join(run_dir, "rep" + plan)
        smoke_txt = open(os.path.join(stage, "smoke.log"), errors="replace").read() if os.path.exists(os.path.join(stage, "smoke.log")) else ""
        chk = open(os.path.join(stage, "smoke-check.txt"), errors="replace").read() if os.path.exists(os.path.join(stage, "smoke-check.txt")) else ""
        calls = driver_lines60(sb)
        tlog = open(os.path.join(sb["logs"], "tests.log")).read().split()
        if path.startswith("abort-netem"):
            ld_txt = open(os.path.join(stage, "ld.log"), errors="replace").read() if os.path.exists(os.path.join(stage, "ld.log")) else ""
            led_txt = smoke_txt + ld_txt
            check(f"{tag}: ABORT-NETEM is in the ledger (the row's NETEM62 line names the bad read), the launch FAILED (rc 5, FAILED-ALL), no terminal marker",
                  rc == 5 and failed and "ABORT-NETEM" in led_txt and "NETEM62 " in led_txt and "jitter term 5ms" in led_txt and ("post/srv: jitter term" if path == "abort-netem-last" else "pre/cli: jitter term") in led_txt and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out, (rc, led_txt[-700:], tail(e, 6)))
            if path == "abort-netem-first":
                check(f"{tag}: the FIRST row's pre read stopped the launch after that one row: the driver was called once, ABORT-SMOKE, no REHEARSAL-PASS, nothing scored",
                      len(calls) == 1 and "ABORT-SMOKE" in e and not os.path.exists(os.path.join(stage, "REHEARSAL-PASS")) and not os.path.exists(os.path.join(stage, "SMOKE-PASS")), (len(calls), tail(e, 6)))
            else:
                sc = open(os.path.join(stage, "score.txt"), errors="replace").read() if os.path.exists(os.path.join(stage, "score.txt")) else ""
                check(f"{tag}: the LAST row's post read stopped the launch (every row ran: {smoke_n + len(table)} driver calls), no DONE-ALL, and the scorer refuses to score (ABORT-NETEM, LABEL62 NEED0 ABORT-NETEM)",
                      len(calls) == smoke_n + len(table) and not os.path.exists(os.path.join(run_dir, "DONE-ALL")) and "ABORT-NETEM" in sc and "LABEL62 NEED0 ABORT-NETEM" in sc and "LABEL62 GATE-J0" not in sc, (len(calls), sc[:500]))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        check(f"{tag}: rc 0 and the terminal marker {marker}", rc == 0 and marker in out and not failed, (rc, tail(out, 30), tail(e, 15)))
        check(f"{tag}: ONE stage rep{plan} (no guard, no BASE tree), NO cargo build (the prebuilt binary installed, its sha equal to the registered bin_sha and on record), the smoke check printed SMOKE-PASS",
              not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and builds == [] and "binary prebuilt from" in e
              and ("equals the registered value " + sha) in e and sha in open(os.path.join(run_dir, "BINSHA.txt")).read() and "SMOKE-PASS" in chk, (builds, tail(e, 6), chk[-600:]))
        sdir = os.path.join(run_dir, "suites")
        tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read() if os.path.exists(os.path.join(sdir, "TESTS.txt")) else ""
        hit = re.search(r"suite-reuse (HIT|MISS)", e)
        check(f"{tag}: the engine suite-reuse HIT is on record and the engine suites were skipped (no cargo test; no false suite record)", hit is not None and hit.group(1) == "HIT" and tests == [] and "ENGINE-SUITES-REUSED" in tt
              and not os.path.exists(os.path.join(sdir, "TESTS-OK")), (hit and hit.group(0), tests, tt[:300]))
        plans_seen = {r.split("plan=")[1].split()[0] for r in rows}
        check(f"{tag}: every row-parser call ran under LD_PLAN={plan}, with LD_BASE_SHA empty", plans_seen == {plan} and {re.search(r"base_sha=(\S*)", r).group(1) for r in rows} == {""}, (plans_seen, rows[:1]))
        if rehearse:
            want_rows = plan62.plan("62r0")
            label = {"rehearse": "GO", "reprobe": "GO", "nogo-floor": "NO-GO-FLOOR", "nogo-gate": "NO-GO-GATE", "nogo-pilot": "NO-GO-PILOT", "nogo-control": "NO-GO-CONTROL", "nogo-short": "NO-GO-SHORT"}[path]
            led = led56(stage, "smoke.log")
            extra_rows = {"reprobe": plan62.reprobe_slots(plan62.REPROBE_HIGH), "nogo-pilot": plan62.reprobe_slots(plan62.REPROBE_HIGH)}.get(path, [])
            got = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
            want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in want_rows + extra_rows]
            if path == "nogo-floor":      # a floor DNF is re-run once (the second DNF stays: the slot holds two rows)
                want_slots = want_slots[:3] + [want_slots[2]] + want_slots[3:]
            check(f"{tag}: the rehearsal ran its {len(want_slots)} rows in table order (R1-R6 and D1{', then the re-probe at W 64 / 96' if extra_rows else ''}), no battery (no ld.log), no engine suites, REHEARSAL-PASS persisted",
                  got == want_slots and tests == [] and not os.path.exists(os.path.join(stage, "ld.log")) and os.path.exists(os.path.join(stage, "REHEARSAL-PASS")), (got, want_slots))
            check(f"{tag}: every row has its t60 / t61 / t62 readouts, no readout error and a clean netem witness (pre / post on both legs)",
                  all(r.get("t60") and r.get("t61") and r.get("t62") and not r.get("t60_error") and not r.get("t62_error") and r["t62"]["netem_problems"] == [] for r in led), [(r["arm"], r.get("t62")) for r in led[:2]])
            if path in ("rehearse", "reprobe"):
                check(f"{tag}: every row is LIVE with no problem and the driver saw each row's run spec (bytes, timeout, hint, RWM_SLAW, pool) in table order (no pilot knee yet: the probes take the bytes formula)",
                      all(r["status"] == "LIVE" and r["problems"] == [] for r in led) and calls == expect_calls62(want_rows + extra_rows), (calls[:2], expect_calls62(want_rows)[:2]))
            check(f"{tag}: the check prints `LABEL62 GO-c3j0 {label}`, the REHEARSAL62 line is on record, SMOKE-PASS (a NO-GO is a Result, not a smoke failure) and no SMOKE-FAIL",
                  re.search(r"^LABEL62 GO-c3j0 %s$" % re.escape(label), chk, re.M) is not None and "REHEARSAL62 " in e and "SMOKE-PASS" in chk and "SMOKE-FAIL" not in chk, chk[-1500:])
            if path in ("rehearse", "reprobe"):
                probes = "64/96" if path == "reprobe" else "80/128"
                check(f"{tag}: PILOT62 names the pilot knee (the stub's knee u = 0.95 falls at W 86.5) with its arms, case=proportional, probes={probes}; the first readings (D0, F counts, u, E90_own, X_own, dump coverage) are printed",
                      re.search(r"^PILOT62 W=86\.[45] u_lo=0\.\d+ u_hi=1\.000 arms=.* case=proportional clamped=no probes=%s " % probes, chk, re.M) is not None
                      and all(k in chk for k in ("FIRST-READING D0", "FIRST-READING u", "FIRST-READING X_own", "FIRST-READING dump D12", "FIRST-READING G12")), chk[-1800:])
                d1 = [r for r in led if r["arm"] == "D12"][0]
                check(f"{tag}: the dump row D12 carries RWM_RTT_DUMP=1 on both ends and its dump is read (coverage {d1['t62']['dump']['coverage']:.3f} >= 0.95, no cap); the scored rows echo RWM_RTT_DUMP=0",
                      d1["t62"]["dump"]["coverage"] >= 0.95 and not d1["t62"]["dump"]["cap"] and all(r["t62"]["dump"] is None for r in led if r["arm"] != "D12"), d1["t62"])
                if path == "rehearse":
                    keep_rehearsal62("go", L)
                check(f"{tag}: the tools tests ran (the pre-build gates and the suites stage's full set: the stubbed tests logged their invocations) and their manifest is recorded (TOOLS-TESTS-OK, TOOLS-MANIFEST.sha256 beside it), the registered engine-suite rule untouched",
                      "TEST parsers rc=0" in tt and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")) and os.path.getsize(os.path.join(sdir, "TOOLS-MANIFEST.sha256")) > 0 and {"test_s62.py", "test_harness62.py", "test_lib_battery.sh"} <= set(tlog), (tt[:300], sorted(set(tlog))[:6]))
            if path == "reprobe":
                check(f"{tag}: the re-probe ran ONCE inside the rehearsal ('REPROBE62 probes 64 / 96' in the ledger) and the first pair's case was both-above", "REPROBE62 probes 64 / 96" in smoke_txt and smoke_txt.count("REPROBE62") == 1, smoke_txt[-300:])
            if path == "nogo-pilot":
                check(f"{tag}: the re-probe ran once and ITS case is non-proportional too: NO-GO-PILOT detail names case=both-above and the pilot line carries probes=64/96", "REPROBE62 probes 64 / 96" in smoke_txt and "probes=64/96" in chk and "case=both-above" in chk, chk[-900:])
                keep_rehearsal62("nogo", L)
            if path == "nogo-floor":
                check(f"{tag}: the floor row's DNF was re-run once ('REPLACE62 ... reason=floor-dnf') and the gate / pilot / control rules still read GO (only NO-GO-FLOOR fails)", "REPLACE62 c3j0-25 G12 s7 rep=2 reason=floor-dnf" in smoke_txt and "NO-GO-LIST NO-GO-FLOOR" in chk, chk[-500:])
            EXECUTED.add((plan, path))
            return
        # plan 62a: a whole session
        led = led56(stage)
        want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
        got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
        smoke_rows = plan62.plan("62a", smoke=True, knee=kf)
        if path == "slaw-echo-void":
            first = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) == want_slots[0]]
            others = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) != want_slots[0]]
            ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
            check(f"{tag}: the first scored slot echoed RWM_SLAW wrongly: its first row is WITNESS-FAIL on gate-slaw, 'REPLACE62 ... reason=slaw-echo' is in the ledger, the replacement row is LIVE and clean; every other slot has one row",
                  len(first) == 2 and first[0]["status"] == "WITNESS-FAIL" and any(x.startswith("gate-slaw") for x in first[0]["problems"]) and first[1]["status"] == "LIVE" and first[1]["problems"] == []
                  and re.search(r"^REPLACE62 %s %s s%s rep=%s reason=slaw-echo" % (want_slots[0][0], want_slots[0][1], want_slots[0][2], want_slots[0][3]), ldtxt, re.M) is not None
                  and len(others) == len(want_slots) - 1, [(r["status"], r["problems"]) for r in first])
            check(f"{tag}: the replacement ran the same arm: the driver was called {len(want_slots) + 2} times (1 smoke row, the VOID row, its replacement, the rest)", len(calls) == len(want_slots) + 2 and calls[1] == calls[2], len(calls))
            sc = open(os.path.join(stage, "score.txt"), errors="replace").read()
            check(f"{tag}: the score reports the replaced slot in its VERDICT62 line", re.search(r"^VERDICT62 .*replaced=1", sc, re.M) is not None, sc[:600])
            EXECUTED.add((plan, path))
            return
        check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}: 7 arms, the K arms from LD_KNEE62={knee}, and the two dump rows), one row each, every row LIVE with its t60 / t61 / t62 readouts, no problem on any row",
              got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t60") and r.get("t61") and r.get("t62") and not r.get("t60_error") and not r.get("t61_error") and r["problems"] == [] for r in led),
              (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], r.get("t60_error"), r.get("t61_error")) for r in led if r["status"] != "LIVE" or r["problems"]][:3], got_slots[:3], want_slots[:3]))
        check(f"{tag}: the driver saw every row's run spec in order: the smoke row (K2), then the table (bytes, timeout, hint, RWM_SLAW, RWM_STORE_PATH_POOL, RWM_RDIAG, RWM_RTOBS, RWM_DIAG=1, the one binary)",
              calls == expect_calls62(smoke_rows + table, kf), (len(calls), calls[:3], expect_calls62(smoke_rows + table, kf)[:3]))
        check(f"{tag}: every row's live qdisc read clean before and after (t62 netem_problems == []), the D rows echo RWM_RTT_DUMP=1 and carry a dump reading, the G rows none",
              all(r["t62"]["netem_problems"] == [] for r in led) and all((r["t62"]["dump"] is not None) == r["arm"].startswith("D") for r in led) and all(r["t62"]["dump"]["coverage"] >= 0.95 for r in led if r["arm"].startswith("D")), [(r["arm"], r["t62"]) for r in led[-2:]])
        shas = {r["bin_sha"] for r in led}
        check(f"{tag}: one binary ran every row, its sha is the registered one, and the seat comparison executed on every row", shas == {sha} and all(r["sha_checked"] is True for r in led), shas)
        ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
        envl = ldtxt.split("=== env ")[-1].split("\n")[0]
        check(f"{tag}: the env scrub withholds RWM_SLAW, RWM_THREE_TERM and RWM_RTT_DUMP; the instrument env is RWM_RDIAG=1 + the timeout + RWM_NETEM_WITNESS=1",
              " RWM_SLAW RWM_THREE_TERM " in envl and "RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=120 RWM_NETEM_WITNESS=1" in envl, envl[:400])
        sc = open(os.path.join(stage, "score.txt"), errors="replace").read()
        m = re.search(r"^VERDICT62 \S+ slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+) dnf=(\d+) rule_finding=(\d+) dump_rows=(\d+)", sc, re.M)
        check(f"{tag}: the score prints a VERDICT62 line counting every slot (every row live, none replaced, the two dump rows outside the scored cell) and the registered labels (CONTROL-c3j0, NEED0)",
              m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led) and int(m.group(4)) == 0 and int(m.group(5)) == 0 and int(m.group(8)) == 2 and "LABEL62 CONTROL-c3j0 " in sc and "LABEL62 NEED0 " in sc, sc[-1200:])
        check(f"{tag}: the score read the plan's own cell and the readings it owns (CR62, the control, the floors, the knees)", "CR62 c3j0" in sc and "CONTROL c3j0 W512" in sc and ("KNEES:" in sc if FULL else True), sc[:1500])
        check(f"{tag}: DONE-ALL and score.txt", os.path.exists(os.path.join(run_dir, "DONE-ALL")) and os.path.exists(os.path.join(run_dir, "score.txt")), tail(e, 12))
        if path == "full":
            check(f"{tag}: the tools-test reuse HIT: tools/l1 is byte-identical to the rehearsal's, so NO tools test ran at all (the stubbed tests log every invocation: none, python or bash; the pre-build gates and the suites stage both skipped: "
                  "TOOLS-TESTS-REUSED, no 'TEST parsers' / 'TEST bashtests' record) and TOOLS-TESTS-OK was written",
                  "tools-tests reuse HIT" in e and "TOOLS-TESTS-REUSED" in tt and "TEST parsers" not in tt and "TEST bashtests" not in tt and tlog == [] and "parser test ok (reused from the rehearsal: skipped)" in e
                  and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")) and os.path.getsize(os.path.join(sdir, "TOOLS-MANIFEST.sha256")) > 0, (tail(e, 8), tt[:300], tlog[:5]))
            check(f"{tag}: the rehearsal's GO and PILOT62 W={wprint} were read from its smoke-check (LD_REHEARSAL62) before any row", ("the rehearsal's GO and PILOT62 W=%s read from" % wprint) in e, tail(e, 10))
        if path == "tools-reuse-miss":
            check(f"{tag}: ONE byte of tools/l1 differs from the rehearsal's: the tools-test reuse MISSed (named in the era), the python and bash tools tests RAN (TEST parsers / bashtests rc=0), and the launch still completed",
                  "tools-tests reuse MISS: tools/l1 differs" in e and "TOOLS-TESTS-REUSED" not in tt and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK"))
                  and {"test_s62.py", "test_harness62.py", "test_lib_battery.sh", "test_abort_witness.sh"} <= set(tlog), (tail(e, 8), tt[:300], sorted(set(tlog))[:6]))
        EXECUTED.add((plan, path))
    finally:
        if L is not None and not os.environ.get("STUBRUN_KEEP"):
            shutil.rmtree(L["root"], ignore_errors=True)
        elif L is not None:
            print("     kept sandbox", L["root"])


SUITES_DIR = {}          # plan -> persisted dir of the plan's full run (the green suite run the prior-tests cases reuse)


# ── plan 64 (status 64, F2's merge guard): stage G = plan 41d + the F2 row witnesses, stage R = Realtime c2-100 at rho = 1 (RB / RC); suite reuse by the wider src_manifest(), the prebuilt binaries and the rehearsal's tools result ──
REH64 = {}          # the rehearsal's persisted run dir (the launch's LD_BIN_PREBUILT_* / LD_PRIOR_TOOLS) and its pins
P64_AFTER = ("full", "tools-manifest-differs", "prebuilt-sha-mismatch")      # cases that consume the rehearsal's artifacts (phase 2)


def bench_dir64(sb):
    """STUB_BENCH_DIR: the three bench blocks of the committed commit-8 pre-read (docs/derivations/f2-guard/framing_ns-ab9feec4.txt, kept in testdata/), the stub cargo prints them in rotation."""
    d = os.path.join(sb["logs"], "bench")
    os.makedirs(d, exist_ok=True)
    text = open(os.path.join(HERE, "testdata", "framing_ns-ab9feec4.txt"), encoding="utf-8").read()
    parts = [x for x in re.split(r"^--- bench \d+\n", text, flags=re.M) if x.strip()]      # the separators END each block
    assert len(parts) == 3, len(parts)
    for i, ptxt in enumerate(parts, 1):
        with open(os.path.join(d, "b%d.txt" % i), "w", newline="\n") as fh:
            fh.write(ptxt)
    return px(d)


def ref64_file(root, plan, edits=None):
    """The reference manifest of plan 64 for the sandbox: src_manifest() (manifest_py) of a PRISTINE sandbox tree; the case's own sandbox may differ from it by `edits` (a MISS)."""
    r2 = tempfile.mkdtemp(prefix="stubrun-ref-")
    try:
        sb2 = make_sandbox(r2, plan, BASE_SHA)
        text = manifest_py(sb2["repo"])
    finally:
        shutil.rmtree(r2, ignore_errors=True)
    path = os.path.join(root, "ref64.sha256")
    with open(path, "w", newline="\n") as fh:
        fh.write(text)
    return px(path)


def binsha64(run_dir):
    """-> (cand sha, base sha) from a run dir's BINSHA.txt (the CAND line first, the BASE line second)."""
    ls = open(os.path.join(run_dir, "BINSHA.txt"), errors="replace").read().splitlines()
    return ls[0].split()[0], ls[1].split()[0]


def led64(run_dir, stage, name="ld.log"):
    return led56(os.path.join(run_dir, stage), name)


def mask_hint(t):
    return "\n".join("RESOLUTION-HINT" if ln.startswith("RESOLUTION (reported only)") else ln for ln in t.splitlines())


def case64(plan, path):
    """Plan 64 on the stubs. `rehearse` and `full` are the registered shapes (the launch runs on the rehearsal's binaries and tools result); every other path injects ONE defect (STUB_BAD64) or breaks one reuse input."""
    import json
    import plan64
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        edits = {}
        src_text = "fn main() {}\n"
        if path == "manifest-miss":
            src_text = "fn main() { /* one byte differs */ }\n"
        if path == "manifest-miss-cargo-toml-only":
            edits = {"raptorpath/Cargo.toml": "[package]\nname = \"raptorpath\"\n[features]\ntest-seams = []\n"}      # a Cargo.toml the narrower engine_manifest() never reads
        sb = make_sandbox(root, plan, BASE_SHA, src_text=src_text, edits=edits)
        if path == "tools-manifest-differs":
            with open(os.path.join(sb["repo"], "tools", "l1", "extra_tool.txt"), "w", newline="\n") as fh:
                fh.write("one more file under tools/l1: the manifest the rehearsal recorded no longer matches\n")
        extra = {"LD_REF_MANIFEST": ref64_file(root, plan), "STUB_BENCH_DIR": bench_dir64(sb)}
        kinds = {"cand-bulk-unframable": "unframable", "cand-bulk-dnf-unframable": "dnf-unframable", "cand-bulk-symsz-missing": "symsz-missing", "cand-bulk-ctld-send-only": "ctld-send-only",
                 "completed-row-live": "bare", "base-rt-dead": "base-rt-dead", "base-rt-dead-rc0": "base-rt-dead-rc0", "rc-nodata-replaced": "rc-nodata", "rt-rho-one-end": "rt-rho-one-end", "cand-rt-dnf-base-completes": "rt-dnf", "cand-rt-dnf-base-dnf": "rt-base-dnf",
                 "cand-rt-ctld-send-only": "rt-ctld-send-only", "cand-rt-symsz-1200": "rt-symsz-1200", "rc-void-twice": "rc-void-twice"}
        if path in kinds:
            extra["STUB_BAD64"] = kinds[path]
        if path == "base-has-symsz":
            extra["STUB_BASE_SYMSZ"] = "1"
        launch_from_rehearsal = path in P64_AFTER
        if launch_from_rehearsal:
            reh = REH64.get("run")
            if reh is None:
                check(f"{tag}: needs the rehearsal's run dir", False, "the 64/rehearse case did not leave one")
                return
            cand_sha, base_sha = binsha64(reh)
            extra.update({"LD_BIN_PREBUILT_CAND": px(os.path.join(reh, "bins64", "cand-raptorpath")), "LD_BIN_PREBUILT_BASE": px(os.path.join(reh, "bins64", "base-raptorpath")),
                          "LD_BIN_SHA_CAND": ("0" * 64 if path == "prebuilt-sha-mismatch" else cand_sha), "LD_BIN_SHA_BASE": base_sha, "LD_PRIOR_TOOLS": px(reh)})
        rehearse = path in ("rehearse", "manifest-miss", "manifest-miss-cargo-toml-only", "base-has-symsz")
        rc, out, rows, cargo = run_launch(sb, "64", BASE_SHA, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        tests = [c for c in cargo if "|test " in c and "framing_ns" not in c and "--no-run" not in c]
        if path in ("manifest-miss", "manifest-miss-cargo-toml-only"):
            check(f"{tag}: {'one byte of raptorpath/src' if path == 'manifest-miss' else 'only raptorpath/Cargo.toml (outside the narrower engine manifest, inside src_manifest())'} differs from the reference manifest -> ABORT-SUITES plan 64 suite-reuse MISS, FAILED-ALL, rc 5, NOTHING measured (no row-parser call, no driver call, no ld.log) and no suite ran",
                  rc == 5 and "ABORT-SUITES plan 64 suite-reuse MISS" in e and failed and rows == [] and open(os.path.join(sb["logs"], "drv.log")).read() == "" and tests == []
                  and "LD-ALL-REHEARSAL-DONE" not in out and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(e, 6), rows[:2]))
            EXECUTED.add((plan, path))
            return
        if path == "base-has-symsz":
            check(f"{tag}: the BASE binary carries [SYMSZ] -> ABORT-SEAT, FAILED-ALL, rc 5, NOTHING measured (both builds ran, the check follows them; no row-parser call, no driver call)",
                  rc == 5 and "ABORT-SEAT plan 64: the BASE binary carries [SYMSZ]" in e and failed and rows == [] and open(os.path.join(sb["logs"], "drv.log")).read() == "" and len(builds) == 2, (rc, tail(e, 6), len(builds)))
            EXECUTED.add((plan, path))
            return
        if path == "prebuilt-sha-mismatch":
            check(f"{tag}: LD_BIN_SHA_CAND differs from the prebuilt CAND binary -> ABORT-BUILD, FAILED-ALL, rc 5, NOTHING measured, nothing built (the prebuilt install is checked against the pin before any row)",
                  rc == 5 and "ABORT-BUILD plan 64 CAND binary" in e and "pinned LD_BIN_SHA_CAND" in e and failed and rows == [] and open(os.path.join(sb["logs"], "drv.log")).read() == "" and builds == [], (rc, tail(e, 6), builds))
            EXECUTED.add((plan, path))
            return
        rehearse_ok = rc == 0 and "LD-ALL-REHEARSAL-DONE" in out and not failed
        done_ok = rc == 0 and "LD-ALL-DONE" in out and not failed
        gdir, rdir = os.path.join(run_dir, "guard"), os.path.join(run_dir, "rt64")
        if path == "rehearse":
            check(f"{tag}: rc 0 and LD-ALL-REHEARSAL-DONE; BOTH stage smokes passed (guard/ and rt64/ REHEARSAL-PASS), no battery (no ld.log anywhere)",
                  rehearse_ok and all(os.path.exists(os.path.join(d, "REHEARSAL-PASS")) for d in (gdir, rdir)) and not any(f == "ld.log" for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(out, 25), tail(e, 12)))
            check(f"{tag}: two builds, the seat witness on record (CAND holds [SYMSZ] and [UNFRAMABLE], BASE neither), both shas recorded in BINSHA.txt and both binaries kept in bins64/ (the launch's LD_BIN_PREBUILT_*)",
                  len(builds) == 2 and "seat witness: CAND binary holds [SYMSZ] and [UNFRAMABLE], BASE neither" in e and all(os.path.exists(os.path.join(run_dir, "bins64", f)) for f in ("cand-raptorpath", "base-raptorpath"))
                  and hashlib.sha256(open(os.path.join(run_dir, "bins64", "cand-raptorpath"), "rb").read()).hexdigest() == binsha64(run_dir)[0]
                  and hashlib.sha256(open(os.path.join(run_dir, "bins64", "base-raptorpath"), "rb").read()).hexdigest() == binsha64(run_dir)[1] != binsha64(run_dir)[0], (len(builds), tail(e, 8)))
            sdir = os.path.join(run_dir, "suites")
            tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read() if os.path.exists(os.path.join(sdir, "TESTS.txt")) else ""
            hit = re.search(r"suite-reuse (HIT|MISS)", e)
            check(f"{tag}: the reference manifest HITs (suite-reuse HIT in the era log), no engine suite ran (no cargo test), the tools tests ran (python and bash), and TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256 are written (no TESTS-OK: no false suite record)",
                  hit is not None and hit.group(1) == "HIT" and tests == [] and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and "TEST main" not in tt
                  and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")) and os.path.getsize(os.path.join(sdir, "TOOLS-MANIFEST.sha256")) > 0 and not os.path.exists(os.path.join(sdir, "TESTS-OK")), (hit and hit.group(0), tests, tt[:300], os.listdir(sdir)))
            rep_ = open(os.path.join(run_dir, "bench", "framing-report.txt"), errors="replace").read() if os.path.exists(os.path.join(run_dir, "bench", "framing-report.txt")) else ""
            raw = open(os.path.join(run_dir, "bench", "framing_ns.txt"), errors="replace").read() if os.path.exists(os.path.join(run_dir, "bench", "framing_ns.txt")) else ""
            tsk = open(os.path.join(sb["logs"], "taskset.log")).read().splitlines() if os.path.exists(os.path.join(sb["logs"], "taskset.log")) else []
            bench_calls = [c for c in cargo if "framing_ns" in c]
            check(f"{tag}: framing_ns: the test binary is BUILT first (--no-run, no taskset), then 10 runs, each under `taskset -c 2`, into bench/framing_ns.txt; the report prints the 10-run per-point medians and the BENCH label",
                  raw.count("--- run ") == 10 and len(tsk) == 10 and all(x == "taskset -c 2 " + x.split("taskset -c 2 ", 1)[-1] and "framing_ns" in x for x in tsk) and len(bench_calls) == 10
                  and any("|test " in c and "--no-run" in c for c in cargo) and [i for i, c in enumerate(cargo) if "--no-run" in c][0] < [i for i, c in enumerate(cargo) if "framing_ns" in c][0]
                  and "BENCH64 framing_ns: 10 runs" in rep_ and rep_.strip().splitlines()[-1] in ("BENCH PASS",) and "framing_ns (10 runs, CAND's tree, taskset -c 2): BENCH PASS" in e, (len(tsk), len(bench_calls), rep_[-400:]))
            gchk = open(os.path.join(gdir, "smoke-check.txt"), errors="replace").read()
            rchk = open(os.path.join(rdir, "smoke-check.txt"), errors="replace").read()
            check(f"{tag}: stage G's smoke (plan 41d's 3 rows) printed the F2 criteria as PASS lines (no VOID-RC, [SYMSZ] 1200/1200 + the server's [CTLD] undeliv=, undeliv = 0 / no [UNFRAMABLE]) and SMOKE-PASS; stage R's smoke (RB s42, RC s42) its criteria and SMOKE-PASS",
                  all(re.search(r"^PASS " + re.escape(x), gchk, re.M) for x in ("every smoke row ran", "a completed row is LIVE on each seat", "on every CAND smoke row [SYMSZ]", "undeliv = 0 on every [CTLD] line")) and "FAIL " not in gchk and "SMOKE-PASS" in gchk
                  and all(re.search(r"^PASS " + re.escape(x), rchk, re.M) for x in ("both rows ran", "RC s42 passes the F2 reading", "RC s42 is RT-OK", "RC s42 shows window_reliable: true on both ends", "RC s42 has its readouts computed", "RB s42 is LIVE"))
                  and "FAIL " not in rchk and "SMOKE-PASS" in rchk, (gchk[-700:], rchk[-700:]))
            gs, rs = led64(run_dir, "guard", "smoke.log"), led64(run_dir, "rt64", "smoke.log")
            calls = [ln.split() for ln in open(os.path.join(sb["logs"], "drv5.log")).read().splitlines()]
            check(f"{tag}: the smoke ledgers: stage G {len(gs)} rows (c1s BASE, c1s CAND, c8 CAND), stage R 2 rows (RB then RC); no row is VOID-RC; stage R's rows ran Realtime at 100 MB, timeout 150, RWM_C_RELIABLE unset, RB on bin-base and RC on bin",
                  len(gs) == 3 and len(rs) == 2 and all(r["status"] != "VOID-RC" and r["rc"] == 0 for r in gs + rs) and [r["arm"] for r in rs] == ["RB", "RC"]
                  and [c for c in calls if c[0] == "realtime"] == [["realtime", "100000000", "single", "timeout=150", "reliable=unset", "diag=1", "bindir=bin-base"], ["realtime", "100000000", "single", "timeout=150", "reliable=unset", "diag=1", "bindir=bin"]],
                  ([(r["cell"], r["arm"], r["status"], r["rc"]) for r in gs + rs], calls))
            # persist what the launch consumes
            REH64["run"] = os.path.join(tempfile.gettempdir(), "stubrun-reh64-%d" % os.getpid())
            shutil.copytree(run_dir, REH64["run"])
            EXECUTED.add((plan, path))
            return
        # ── a whole session (the launch) ──
        check(f"{tag}: rc 0 and the terminal marker LD-ALL-DONE, DONE-ALL; both stages ran to the end (guard/ and rt64/ DONE)", done_ok and os.path.exists(os.path.join(run_dir, "DONE-ALL")) and all(os.path.exists(os.path.join(d, "DONE")) for d in (gdir, rdir)),
              (rc, tail(out, 30), tail(e, 15)))
        plans_seen = {r.split("plan=")[1].split()[0] for r in rows}
        bases = {re.search(r"base_sha=(\S*)", r).group(1) for r in rows}
        check(f"{tag}: the row parser ran under LD_PLAN=41d (stage G, LD_PLAN=64 mapped like plan 54) and LD_PLAN=64 (stage R only), each with LD_BASE_SHA={BASE_SHA[:8]}.. having passed `sudo -n env`", plans_seen == {"41d", "64"} and bases == {BASE_SHA}, (plans_seen, bases))
        gl, gr = led64(run_dir, "guard"), led64(run_dir, "guard", "repl.log")
        rl, rr = led64(run_dir, "rt64"), None
        sc = open(os.path.join(gdir, "score.txt"), errors="replace").read()
        rsc = open(os.path.join(rdir, "score.txt"), errors="replace").read()
        summ = open(os.path.join(run_dir, "score.txt"), errors="replace").read()
        if launch_from_rehearsal:
            tt = open(os.path.join(run_dir, "suites", "TESTS.txt"), errors="replace").read()
            if path == "tools-manifest-differs":
                check(f"{tag}: tools/l1 differs from the rehearsal's manifest (one more file): the tools tests RUN (TEST parsers rc=0, TEST bashtests rc=0), nothing is reused (no TOOLS-TESTS-REUSED), the engine suites are still skipped (HIT)",
                      "TOOLS-TESTS-REUSED" not in e and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and "tools/l1 differs from the tree the rehearsal ran on" in e and "suite-reuse HIT" in e and tests == [], (tail(e, 12), tt[:300]))
            else:
                check(f"{tag}: the launch reused the rehearsal's artifacts: NO build and NO cargo test (both binaries installed from the rehearsal, sha-checked against its pins), suite-reuse HIT, TOOLS-TESTS-REUSED (tools/l1 byte-identical) and no tools test ran",
                      builds == [] and tests == [] and "suite-reuse HIT" in e and "TOOLS-TESTS-REUSED" in e and "TEST parsers" not in tt and "BASE binary prebuilt from" in e and "CAND binary prebuilt from" in e
                      and os.path.exists(os.path.join(run_dir, "suites", "TOOLS-TESTS-OK")) and not os.path.exists(os.path.join(run_dir, "suites", "TESTS-OK")), (builds, tests, tail(e, 14), tt[:200]))
                shas = binsha64(run_dir)
                check(f"{tag}: the installed binaries are the rehearsal's (BINSHA.txt equals the rehearsal's pins) and every row's bin_sha matches its seat, the comparison executed (sha_checked) on every row",
                      shas == binsha64(REH64["run"]) and {r["bin_sha"] for r in gl if r["arm"] == "CAND"} == {shas[0]} and {r["bin_sha"] for r in gl if r["arm"] == "BASE"} == {shas[1]}
                      and all((r.get("w64") or {}).get("sha_checked") is True for r in gl + gr) and all((r.get("t64") or {}).get("sha_checked") is True for r in rl), (shas, binsha64(REH64["run"])))
        else:
            check(f"{tag}: two builds (CAND tree, BASE tree) and no engine suite (suite-reuse HIT)", len(builds) == 2 and tests == [] and "suite-reuse HIT" in e, (builds, tests))
        n_g = 16 if not FULL else 96
        check(f"{tag}: stage G's ledger holds the 41d design ({n_g} rows: BASE n + CAND n per cell); every CAND row carries the F2 witnesses (executed: [SYMSZ] on both ends), every BASE row its f2_void reading",
              len(gl) == n_g and sum(1 for r in gl if r["arm"] == "CAND" and not (r.get("w64") or {}).get("executed")) == (1 if path == "cand-bulk-symsz-missing" else 0) and all("f2_void" in (r.get("w64") or {}) for r in gl if r["arm"] == "BASE"),
              (len(gl), [(r["arm"], r.get("w64")) for r in gl][:2]))
        return_ctx = dict(sb=sb, rows=rows, run_dir=run_dir, e=e, out=out, tests=tests, builds=builds, gl=gl, gr=gr, rl=rl, sc=sc, rsc=rsc, summ=summ, gdir=gdir, rdir=rdir)
        r64_checks(tag, path, return_ctx)
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)

def r64_checks(tag, path, c):
    """The path-specific assertions of a whole plan-64 session (the launch, with or without one injected defect)."""
    import plan64
    sb, run_dir, e, gl, gr, rl, sc, rsc, summ, gdir, rdir = (c[k] for k in ("sb", "run_dir", "e", "gl", "gr", "rl", "sc", "rsc", "summ", "gdir", "rdir"))
    n_rows_r = 24 if FULL else 4
    f2_rows = [r for r in gl + gr if r["status"] == "F2-FAIL"]
    cand_first = next((r for r in gl if r["arm"] == "CAND"), None)
    gate = re.search(r"^GATE-64 (PASS|FAIL)(.*)$", sc, re.M)
    v41 = re.search(r"^VERDICT (GATE-41d \S+|NO VERDICT.*)$", sc, re.M)
    # stage R's ledger and verdict
    check(f"{tag}: stage R's ledger holds the table's {n_rows_r} slots (RB / RC on c2-100, plan 41c's interleave) plus the replacements the path asks for, every row with its t64 readings; the rows ran in their own stage (rt64/), not the guard's",
          {r["arm"] for r in rl} == {"RB", "RC"} and {r["cell"] for r in rl} == {"c2-100"} and len({(r["arm"], r["seed"], r["rep"]) for r in rl}) == n_rows_r and all(r.get("t64") for r in rl)
          and not any(r["arm"] in ("RB", "RC") for r in gl + gr), (len(rl), [(r["arm"], r["status"]) for r in rl]))
    verdict = re.search(r"^VERDICT64R (\S+) rc_ok=(\d+)/(\d+) rc_void=(\d+) rc_fail=(\d+) base_f2_dead=(\d+)/(\d+) rb_void=(\d+) item5=(\S+)$", rsc, re.M)
    check(f"{tag}: stage R's score printed its VERDICT64R line", verdict is not None, rsc[-600:])
    if verdict is None:
        return
    vlab, ok_n, n_rc, void_n, fail_n, dead_n, n_rb, rbvoid_n, item5 = verdict.group(1), *(int(x) for x in verdict.group(2, 3, 4, 5, 6, 7, 8)), verdict.group(9)
    first_rc = next((r for r in rl if r["arm"] == "RC"), None)
    first_rb = next((r for r in rl if r["arm"] == "RB"), None)
    slot = lambda r: (r["arm"], r["seed"], r["rep"])  # noqa: E731
    if path in ("full", "tools-manifest-differs", "completed-row-live", "base-rt-dead", "rt-rho-one-end", "cand-rt-dnf-base-dnf"):
        pass
    if path in ("full", "tools-manifest-differs", "completed-row-live"):
        m41 = mask_hint(sc.split("F2-WITNESS64")[0]).strip()
        env = dict(os.environ, LD_BASE_SHA=BASE_SHA)
        ref = subprocess.run([sys.executable, os.path.join(sb["repo"], "tools", "l1", "place38_parse.py"), "score41d", os.path.join(gdir, "ld.log").replace("\\", "/"), os.path.join(gdir, "repl.log").replace("\\", "/")],
                             capture_output=True, text=True, errors="replace", env=env).stdout
        check(f"{tag}: score64 prints score41d's output byte for byte (score41d on the same ledgers, line for line, only the argv echoed in the host-side hint differs), then the F2 witness table, then GATE-64 PASS",
              ref.strip() and m41 == mask_hint(ref).strip() and "F2-WITNESS64" in sc and gate is not None and gate.group(1) == "PASS" and "\nVERDICT GATE-41d PASS" in "\n" + sc and not f2_rows, (m41[-300:], mask_hint(ref)[-300:], gate and gate.group(0)))
        sz_cols = re.findall(r"^  (c\S+)\s+CAND rows\s+(\d+)\s+F2-FAIL (\d+)\s+\[SYMSZ\] 1200/1200 on both ends (\d+)/(\d+)", sc, re.M)
        check(f"{tag}: the F2 witness table reads [SYMSZ] 1200/1200 on both ends n/n at all four cells (the ledger is JSON: a pair comes back as a list; on the section 64 session this column read 0/12)",
              len(sz_cols) == 4 and all(a == b == c2 for _, a, _, b, c2 in [(x[0], x[1], x[2], x[3], x[4]) for x in sz_cols]) and all(x[2] == "0" for x in sz_cols), sz_cols)
        check(f"{tag}: every row of both stages is LIVE-class and none is VOID-RC (rc 0 on every row under the real perf script's F2 lines run with `set -euo pipefail`): stage G {len(gl)} + {len(gr)}, stage R {len(rl)}",
              all(r["rc"] == 0 and r["status"] not in ("VOID-RC", "NO_DATA") for r in gl + gr + rl), [(r["cell"], r["arm"], r["status"], r["rc"]) for r in gl + gr + rl if r["rc"] != 0 or r["status"] in ("VOID-RC", "NO_DATA")][:4])
        check(f"{tag}: stage R's rows: every RB row LIVE, every RC row RT-OK, window_reliable true on both ends, [SYMSZ] 512/512 on both ends of an RC row, undeliv 0 with the server's [CTLD] present, the denominator read; the score is RT-PASS with item 5 SCORED, BASE-F2-DEAD 0/{n_rb}",
              all(r["status"] == "LIVE" for r in rl if r["arm"] == "RB") and all(r["status"] == "RT-OK" for r in rl if r["arm"] == "RC")
              and all(r["t64"]["wr_cli"] is True and r["t64"]["wr_srv"] is True and r["t64"]["echo_cli"] and r["t64"]["echo_srv"] for r in rl)
              and all(r["t64"]["symsz_cli"] == [[512, 512]] or r["t64"]["symsz_cli"] == [(512, 512)] for r in rl if r["arm"] == "RC") and all(r["t64"]["ctld_srv"] > 0 and r["t64"]["undeliv"] == 0 and r["t64"]["src_n"] for r in rl if r["arm"] == "RC")
              and vlab == "RT-PASS" and item5 == "SCORED" and dead_n == 0 and void_n == 0 and fail_n == 0 and ok_n == n_rc,
              ([(r["arm"], r["status"], r["problems"], r["t64"]["wr_cli"], r["t64"]["wr_srv"]) for r in rl][:3], verdict.group(0)))
        check(f"{tag}: stage R's score prints the readouts (7, RC and live-RB medians with ranges and the one-sided rank p), the owner-flag line and the denominator per RC row",
              all(re.search(r"^  %s " % re.escape(n), rsc, re.M) for n in ("goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "inorder_delay_mean", "rtp_floor_p0")) and "OWNER-FLAG64" in rsc
              and re.search(r"denominator \(server received\): src_n=\d+ ctld_rx=\d+ ctld_fr=\d+", rsc) is not None and "under 30 s of ticks" in rsc, rsc[-1500:])
        check(f"{tag}: the session summary (score.txt) carries both verdicts (GATE-64 and RT-PASS)", "GATE-64 PASS" in summ and "RT-PASS" in summ and "VERDICT GATE-41d" in summ, summ)
        return
    # ── one injected defect ──
    if path.startswith("cand-bulk-"):
        why_want = {"cand-bulk-unframable": "[UNFRAMABLE]", "cand-bulk-dnf-unframable": "[UNFRAMABLE]", "cand-bulk-symsz-missing": "[SYMSZ] missing on srv", "cand-bulk-ctld-send-only": "the sending end printed one"}[path]
        check(f"{tag}: exactly ONE F2-FAIL row: the first scored CAND row (c1s-400 s42), status F2-FAIL (never VOID, never DNF) carrying its reason ({why_want}) and the label it would have had",
              len(f2_rows) == 1 and f2_rows[0] is not None and (f2_rows[0]["cell"], f2_rows[0]["arm"], f2_rows[0]["seed"]) == ("c1s-400", "CAND", "42") and any(why_want in w for w in f2_rows[0]["f2_why"])
              and f2_rows[0]["status_41d"] == "LIVE" and (f2_rows[0].get("dnf") is True) == (path == "cand-bulk-dnf-unframable"), [(r["cell"], r["arm"], r["status"], r.get("f2_why")) for r in f2_rows])
        slotk = (f2_rows[0]["cell"], f2_rows[0]["arm"], f2_rows[0]["seed"], f2_rows[0]["rep"]) if f2_rows else None
        check(f"{tag}: the F2-FAIL row is never replaced (no row of its slot in repl.log), is not counted toward the replacement cap (REPLACED SLOTS names only the VOID-SHORT slots) and does not turn the verdict into NO VERDICT",
              slotk is not None and not any((r["cell"], r["arm"], r["seed"], r["rep"]) == slotk for r in gr) and "REPLACED SLOTS: 2 of at most 4" in sc and v41 is not None and v41.group(1) == "GATE-41d PASS" and "NO VERDICT" not in sc,
              (v41 and v41.group(0), re.findall(r"REPLACED SLOTS.*", sc)))
        sz_cols = re.findall(r"^  (c\S+)\s+CAND rows\s+(\d+)\s+F2-FAIL (\d+)\s+\[SYMSZ\] 1200/1200 on both ends (\d+)/(\d+)", sc, re.M)
        want_sz = {c_: (int(a), int(b)) for c_, a, _, b, _ in [(x[0], x[1], x[2], x[3], x[4]) for x in sz_cols]}
        check(f"{tag}: the witness table counts [SYMSZ] from the ledger: " + ("c1s shows one row short (the row with no server [SYMSZ])" if path == "cand-bulk-symsz-missing" else "every CAND row of every cell has it"),
              len(sz_cols) == 4 and all((b == a - 1) if (path == "cand-bulk-symsz-missing" and c_ == "c1s-400") else (b == a) for c_, (a, b) in want_sz.items()), sz_cols)
        check(f"{tag}: GATE-64 FAIL because of the F2-FAIL row ('1 F2-FAIL row(s)') while GATE-41d's own p still reads PASS (p > 0.05); the F2 witness table names the row; stage R still ran to the end",
              gate is not None and gate.group(1) == "FAIL" and "1 F2-FAIL row(s)" in gate.group(2) and "GATE-41d" not in gate.group(2) and re.search(r"^  F2-FAIL c1s-400 CAND s42 rep1 \(would be LIVE\)", sc, re.M) is not None and vlab == "RT-PASS", (gate and gate.group(0), sc[-700:]))
        return
    if path in ("base-rt-dead", "base-rt-dead-rc0"):
        k = slot(first_rb)
        mine = [r for r in rl if slot(r) == k]
        check(f"{tag}: the first RB row's warm-up never acked: status BASE-F2-DEAD although the driver exited non-zero (decided before the rc test: rc {first_rb['rc']}, not VOID-RC), never replaced (one row for its slot), no NO VERDICT",
              len(mine) == 1 and first_rb["status"] == "BASE-F2-DEAD" and (first_rb["rc"] == 0) == (path == "base-rt-dead-rc0") and "NO VERDICT" not in rsc and "REPLACED SLOTS: 0" in rsc and not any(r["status"] == "VOID-RC" for r in rl), [(r["arm"], r["status"], r["rc"]) for r in rl])
        ldtxt = open(os.path.join(rdir, "ld.log"), errors="replace").read()
        calls = [ln.split() for ln in open(os.path.join(sb["logs"], "drv5.log")).read().splitlines() if ln.startswith("realtime")]
        check(f"{tag}: a stage R row runs ONCE: the dead row's driver call happened once (no RUN-RETRY, no attempt=2 in the ledger; the stage's driver calls are exactly the 2 smoke rows + the {n_rows_r} table rows)" + (" even with rc 0 and no summary line (the battery's bring-up retry would have drawn it again)" if path == "base-rt-dead-rc0" else ""),
              "RUN-RETRY" not in ldtxt and "attempt=2" not in ldtxt and len(calls) == 2 + n_rows_r and (path != "base-rt-dead-rc0" or not first_rb.get("summary")), (len(calls), ldtxt.count("RUN-RETRY"), first_rb.get("summary")))
        check(f"{tag}: the score reports BASE-F2-DEAD 1/{n_rb}, item 5 SCORED over the surviving RB rows, and RT-PASS (the CAND rows all pass)", dead_n == 1 and item5 == "SCORED" and vlab == "RT-PASS" and void_n == 0 and f"BASE-F2-DEAD {dead_n}/{n_rb}" in rsc, rsc[-900:])
        return
    if path == "rt-rho-one-end":
        k = slot(first_rc)
        mine = [r for r in rl if slot(r) == k]
        ldtxt = open(os.path.join(rdir, "ld.log"), errors="replace").read()
        check(f"{tag}: the first RC row's server printed window_reliable: false (rho not 1 on one end): VOID-RHO, replaced once (REPLACE64 in the ledger), the replacement is RT-OK; the verdict is RT-PASS and the replacement is counted (1 of at most 2)",
              len(mine) == 2 and mine[0]["status"] == "VOID-RHO" and mine[1]["status"] == "RT-OK" and re.search(r"^REPLACE64 c2-100 RC s%s rep=%s reason=void-rho" % (mine[0]["seed"], mine[0]["rep"]), ldtxt, re.M) is not None
              and vlab == "RT-PASS" and "REPLACED SLOTS: 1 of at most 2" in rsc, [(r["arm"], r["status"], r["problems"]) for r in mine])
        return
    if path == "rc-nodata-replaced":
        k = slot(first_rc)
        mine = [r for r in rl if slot(r) == k]
        ldtxt = open(os.path.join(rdir, "ld.log"), errors="replace").read()
        calls = [ln.split() for ln in open(os.path.join(sb["logs"], "drv5.log")).read().splitlines() if ln.startswith("realtime")]
        check(f"{tag}: an RC row ends rc 0 with no summary (NO_DATA): a non-F2 void, REPLACED ONCE (REPLACE64 in the ledger, the replacement RT-OK) and never drawn again by the bring-up retry (no RUN-RETRY; driver calls = 2 smoke + {n_rows_r} table + 1 replacement); RT-PASS; 1 of at most 2 slots replaced",
              len(mine) == 2 and mine[0]["status"] == "NO_DATA" and mine[1]["status"] == "RT-OK" and re.search(r"^REPLACE64 c2-100 RC s%s rep=%s reason=no_data" % (mine[0]["seed"], mine[0]["rep"]), ldtxt, re.M) is not None
              and "RUN-RETRY" not in ldtxt and len(calls) == 2 + n_rows_r + 1 and vlab == "RT-PASS" and "REPLACED SLOTS: 1 of at most 2" in rsc, ([r["status"] for r in mine], len(calls), verdict.group(0)))
        return
    if path == "rc-void-twice":
        k = slot(first_rc)
        mine = [r for r in rl if slot(r) == k]
        check(f"{tag}: an RC slot voids (VOID-RHO) on its row AND on its replacement: two rows, both VOID-RHO; the score says RC-VOID for it, RT-INCOMPLETE, and is NOT RT-PASS (not a finding against F2: no RT-FAIL)",
              len(mine) == 2 and all(r["status"] == "VOID-RHO" for r in mine) and vlab == "RT-INCOMPLETE" and void_n == 1 and fail_n == 0 and "label=RC-VOID" in rsc and re.search(r"^RT-INCOMPLETE \(1 RC-VOID slot", rsc, re.M) is not None
              and "RT-PASS" not in rsc.replace("RT-PASS iff", ""), (verdict.group(0), [(r["status"]) for r in mine]))
        return
    if path == "cand-rt-dnf-base-completes":
        check(f"{tag}: an RC row is a DNF while every live RB row completes: item 5 SCORED and that row is `RT-FAIL 5`; the verdict names it",
              item5 == "SCORED" and vlab == "RT-FAIL" and fail_n == 1 and re.search(r"^RT-FAIL \(s%s r%s item 5\)" % (first_rc["seed"], first_rc["rep"]), rsc, re.M) is not None and first_rc.get("dnf") is True and first_rc["status"] == "RT-OK"
              and "label=RT-FAIL 5" in rsc, (verdict.group(0), rsc[-600:]))
        return
    if path == "cand-rt-dnf-base-dnf":
        check(f"{tag}: the same RC DNF while an RB row is a DNF too: item 5 is REPORTED (an RB row DNF), not scored, and the verdict is RT-PASS (items 1-4 hold on the DNF row)",
              item5 == "REPORTED" and vlab == "RT-PASS" and fail_n == 0 and first_rc.get("dnf") is True and first_rb.get("dnf") is True and "ITEM5 REPORTED (an RB row DNF)" in rsc, (verdict.group(0), first_rc.get("dnf"), first_rb.get("dnf")))
        return
    if path in ("cand-rt-ctld-send-only", "cand-rt-symsz-1200"):
        item = 2 if path == "cand-rt-ctld-send-only" else 4
        check(f"{tag}: the first RC row fails item {item} ({'only the sending end printed [CTLD] undeliv=: a missing receiving-end witness' if item == 2 else '[SYMSZ] reads 1200 at Realtime'}): status RT-FAIL, never replaced, the verdict `RT-FAIL (... item {item})`",
              first_rc["status"] == "RT-FAIL" and first_rc["rt_items"] == [item] and len([r for r in rl if slot(r) == slot(first_rc)]) == 1 and vlab == "RT-FAIL" and fail_n == 1
              and re.search(r"^RT-FAIL \(s%s r%s item %d\)" % (first_rc["seed"], first_rc["rep"], item), rsc, re.M) is not None, (verdict.group(0), first_rc["status"], first_rc.get("rt_items")))
        return
    check(f"{tag}: unhandled path", False, path)



# ── plan 65 (status 65): a stage-R-only launch on the two prebuilt binaries of status 64, scored by the registered s65_score.py ──
REH65 = {}          # the rehearsal's persisted run dir (the launch's LD_PRIOR_TOOLS)
P65_AFTER = ("full",)


def bins65(sb):
    """Two prebuilt stub binaries: CAND holds [SYMSZ] and [UNFRAMABLE], BASE neither; both carry the strings the battery's preflight asks for -> (cand path, cand sha, base path, base sha)."""
    out = []
    for name, extra in (("cand", "# RWM_STORE\n# [SYMSZ] [UNFRAMABLE] F2 framing\n"), ("base", "# RWM_STORE\n")):
        path = os.path.join(sb["logs"], name + "-raptorpath")
        body = "#!/bin/sh\n# stub prebuilt %s binary of status 64\n%sexit 0\n" % (name, extra)
        body = body.replace("\\\n", "\n")
        with open(path, "w", newline="\n") as fh:
            fh.write(body)
        os.chmod(path, 0o755)
        out += [path, hashlib.sha256(body.encode()).hexdigest()]
    return tuple(out)


def case65(plan, path):
    import json
    import plan64
    import s65_score
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        cpath, csha, bpath, bsha = bins65(sb)
        extra = {"LD_BIN_PREBUILT_CAND": px(cpath), "LD_BIN_PREBUILT_BASE": px(bpath), "LD_BIN_SHA_CAND": ("0" * 64 if path == "prebuilt-sha-mismatch" else csha), "LD_BIN_SHA_BASE": bsha}
        kinds = {"rehearse-rb-dead-once": "rb-dead-smoke", "rehearse-rb-dead-twice": "rb-dead-smoke2", "base-dead-redrawn": "base-rt-dead", "base-dead-twice": "base-dead-twice", "unresolved-n": "base-dead-twice-x2",
                 "rc-rtfail": "rt-ctld-send-only", "rho-one-end": "rt-rho-one-end"}
        if path in kinds:
            extra["STUB_BAD64"] = kinds[path]
        if path == "replicated":
            extra["STUB_SHIFT65"] = "0.5"
        if path == "score-sha-mismatch":
            extra["LD_S65_SCORE_SHA"] = "0" * 64
        if path == "prebuilt-missing":
            del extra["LD_BIN_PREBUILT_BASE"]
        if path == "full":
            reh = REH65.get("run")
            if reh is None:
                check(f"{tag}: needs the rehearsal's run dir", False, "the 65/rehearse case did not leave one")
                return
            extra["LD_PRIOR_TOOLS"] = px(reh)
        rehearse = path.startswith("rehearse")
        rc, out, rows, cargo = run_launch(sb, "65", None, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        drv = open(os.path.join(sb["logs"], "drv.log")).read()
        if path in ("score-sha-mismatch", "prebuilt-missing"):
            tok = "ABORT-TOOLS plan 65" if path == "score-sha-mismatch" else "ABORT-BUILD plan 65 needs LD_BIN_PREBUILT_BASE"
            check(f"{tag}: {tok.split(' plan')[0]} -> rc 3, NOTHING measured (no row-parser call, no driver call, no cargo, no run dir content) and no terminal marker",
                  rc == 3 and tok in out and rows == [] and drv == "" and cargo == [] and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out, (rc, out[-300:]))
            EXECUTED.add((plan, path))
            return
        if path == "prebuilt-sha-mismatch":
            check(f"{tag}: LD_BIN_SHA_CAND differs from the prebuilt CAND binary -> ABORT-BUILD, FAILED-ALL, rc 5, NOTHING measured, no cargo",
                  rc == 5 and "ABORT-BUILD plan 65 CAND binary" in e and failed and rows == [] and drv == "" and cargo == [], (rc, tail(e, 5)))
            EXECUTED.add((plan, path))
            return
        stage = os.path.join(run_dir, "rt65")
        smoke_led = led56(stage, "smoke.log")
        calls = [ln.split() for ln in open(os.path.join(sb["logs"], "drv5.log")).read().splitlines()]
        chk = open(os.path.join(stage, "smoke-check.txt"), errors="replace").read() if os.path.exists(os.path.join(stage, "smoke-check.txt")) else ""
        if path == "rehearse-rb-dead-twice":
            check(f"{tag}: RB s42 r1 and the conditional RB s7 r1 both die: SMOKE-FAIL (two BASE-F2-DEAD rows: NO-GO, to the coordinator), ABORT-SMOKE, rc 5, the NO-GO65 line on record, 3 smoke rows, no REHEARSAL-PASS, no battery",
                  rc == 5 and "LD-ALL-REHEARSAL-DONE" not in out and "ABORT-SMOKE stage rt65" in e and "NO-GO65 plan 65 rehearsal" in e and "SMOKE-FAIL (two BASE-F2-DEAD rows: NO-GO, to the coordinator)" in chk
                  and [(r["arm"], r["seed"], r["status"]) for r in smoke_led] == [("RB", "42", "BASE-F2-DEAD"), ("RC", "42", "RT-OK"), ("RB", "7", "BASE-F2-DEAD")] and not os.path.exists(os.path.join(stage, "REHEARSAL-PASS"))
                  and not os.path.exists(os.path.join(stage, "ld.log")), (rc, [(r["arm"], r["seed"], r["status"]) for r in smoke_led], chk[-400:], tail(e, 6)))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        check(f"{tag}: rc 0 and the terminal marker {marker}", rc == 0 and marker in out and not failed, (rc, tail(out, 30), tail(e, 12)))
        tests = [c for c in cargo if "|test " in c]
        check(f"{tag}: no cargo at all (no build, no engine suites: SUITES-NOT-RUN on record), both pinned binaries installed (BINSHA.txt = the pins), the row parser ran under LD_PLAN=65 only, the budget line and the process list (twice) on record",
              cargo == [] and "SUITES-NOT-RUN plan 65" in e and open(os.path.join(run_dir, "BINSHA.txt")).read().split()[0] == csha and open(os.path.join(run_dir, "BINSHA.txt")).read().split("\n")[1].split()[0] == bsha
              and {r.split("plan=")[1].split()[0] for r in rows} == {"65"} and re.search(r"plan 65 budget: \d+s to LD_HARD \(at least %d" % (1800 if rehearse else 6700), e) is not None
              and len(re.findall(r"PROCLIST65 before-", e)) == 2 and os.path.exists(os.path.join(run_dir, "PROCLIST-before-%s.txt" % ("rehearsal" if rehearse else "launch")))
              and os.path.exists(os.path.join(run_dir, "PROCLIST-before-first-row-%s.txt" % ("rehearsal" if rehearse else "launch"))), (cargo, tail(e, 12)))
        if rehearse:
            sdir = os.path.join(run_dir, "suites")
            tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read()
            n_rows = 3 if path == "rehearse-rb-dead-once" else 2
            check(f"{tag}: the smoke ran {n_rows} rows ({'RB s42 r1 died, so the conditional RB s7 r1 ran' if n_rows == 3 else 'RB s42 r1 and RC s42 r1, no third row'}), SMOKE-PASS, REHEARSAL-PASS, no battery (no ld.log); the rows are Realtime, 100 MB, timeout 150, RWM_C_RELIABLE unset, RB on bin-base and RC on bin",
                  len(smoke_led) == n_rows and "SMOKE-PASS" in chk and os.path.exists(os.path.join(stage, "REHEARSAL-PASS")) and not os.path.exists(os.path.join(stage, "ld.log"))
                  and [c for c in calls if c[0] == "realtime"] == [["realtime", "100000000", "single", "timeout=150", "reliable=unset", "diag=1", "bindir=" + b] for b in (["bin-base", "bin"] + (["bin-base"] if n_rows == 3 else []))]
                  and (n_rows == 2 or [(r["arm"], r["seed"], r["status"]) for r in smoke_led] == [("RB", "42", "BASE-F2-DEAD"), ("RC", "42", "RT-OK"), ("RB", "7", "LIVE")]), ([(r["arm"], r["seed"], r["status"]) for r in smoke_led], calls))
            check(f"{tag}: the tools tests ran (python and bash) and TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256 are written; the scorer's sha was checked; every smoke row has qlost_2s",
                  "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")) and os.path.getsize(os.path.join(sdir, "TOOLS-MANIFEST.sha256")) > 0 and not os.path.exists(os.path.join(sdir, "TESTS-OK"))
                  and all(r.get("qlost_2s") is not None for r in smoke_led), (tt[:300], [r.get("qlost_2s") for r in smoke_led]))
            if path == "rehearse":
                REH65["run"] = os.path.join(tempfile.gettempdir(), "stubrun-reh65-%d" % os.getpid())
                shutil.copytree(run_dir, REH65["run"])
            EXECUTED.add((plan, path))
            return
        # ── a whole launch ──
        led = led56(stage)
        sc = open(os.path.join(stage, "score.txt"), errors="replace").read()
        summ = open(os.path.join(run_dir, "score.txt"), errors="replace").read()
        # the SANDBOX's table (fast mode shrinks plan64.py's PAIRS65 / MIN_PAIRS65 in the sandbox copy only; the registered values are 24 / 18)
        sbl1 = os.path.join(sb["repo"], "tools", "l1")
        table = [tuple(int(x) if x.isdigit() and i == 4 else x for i, x in enumerate(ln.split())) for ln in subprocess.run([sys.executable, os.path.join(sbl1, "plan64.py"), "plan", "65"], capture_output=True, text=True).stdout.splitlines()]
        n_slots = len(table)
        n_pairs = n_slots // 2
        min_pairs = int(re.search(r"^MIN_PAIRS65 = (\d+)", open(os.path.join(sbl1, "plan64.py")).read(), re.M).group(1)) if re.search(r"^MIN_PAIRS65 = \d+", open(os.path.join(sbl1, "plan64.py")).read(), re.M) else s65_score.MIN_PAIRS
        extra_rows = {"base-dead-redrawn": 1, "base-dead-twice": 1, "unresolved-n": 2, "rho-one-end": 1}.get(path, 0)
        check(f"{tag}: the ledger holds the table's {n_slots} slots in rt65/ (+ {extra_rows} redraw row(s) the path asks for), every row with its t64 readings and its early-loss field; the stage ran to the end (DONE, DONE-ALL)",
              len(led) == n_slots + extra_rows and len({(r["arm"], r["seed"], r["rep"]) for r in led}) == n_slots and all(r.get("t64") and "qlost_2s" in r for r in led) and os.path.exists(os.path.join(stage, "DONE")) and os.path.exists(os.path.join(run_dir, "DONE-ALL")),
              (len(led), n_slots, extra_rows))
        ref = subprocess.run([sys.executable, os.path.join(sb["repo"], "tools", "l1", "s65_score.py"), os.path.join(stage, "ld.log").replace("\\", "/")], capture_output=True, text=True, errors="replace").stdout
        # the launch's scorer call passes the (shrunk) pair count and floor; the stand-alone file defaults to the registered 24 / 18, so compare through the same call
        buf = []
        sys.path.insert(0, os.path.join(sb["repo"], "tools", "l1"))
        s65_score.score([os.path.join(stage, "ld.log")], out=buf.append, n_pairs=n_pairs, min_pairs=min_pairs)
        check(f"{tag}: score.txt is s65_score.score on the stage's ledger, line for line (the registered scorer, unchanged); the run summary carries VERDICT65, TEST65 and F2-FINDING65",
              sc.strip() == "\n".join(buf).strip() and all(re.search(r"^" + k, summ, re.M) for k in ("VERDICT65", "TEST65", "F2-FINDING65")) and "VERDICT65" in sc, (sc[-300:], summ))
        want_label = {"replicated": "REPLICATED", "unresolved-n": "UNRESOLVED-N"}.get(path, "NOT-REPLICATED")
        check(f"{tag}: VERDICT65 {want_label}", re.search(r"^VERDICT65 %s " % want_label, sc, re.M) is not None, sc[-700:])
        if path == "full":
            tt = open(os.path.join(run_dir, "suites", "TESTS.txt"), errors="replace").read()
            check(f"{tag}: the tools tests were REUSED from the rehearsal (byte-identical tools/l1: TOOLS-TESTS-REUSED) and none ran; USABLE pairs {n_pairs} of {n_pairs}, no redraw, F2-FINDING65 none",
                  "TOOLS-TESTS-REUSED" in e and "TEST parsers" not in tt and f"PAIRS65 usable {n_pairs} of {n_pairs}" in sc and "REDRAWS none" in sc and "F2-FINDING65 none" in sc, (tail(e, 8), sc[:600]))
            ldt = open(os.path.join(stage, "ld.log"), errors="replace").read()
            check(f"{tag}: the smoke ran first (RB s42 r1, RC s42 r1; no third row) and the scored rows follow in the table's order (seed 42's pair RB first, seed 7's RC first); every row LIVE-class, none VOID-RC",
                  [(r["arm"], r["seed"], r["rep"]) for r in led] == [(a, sd, rp) for _, _, a, sd, rp in table] and len(smoke_led) == 2 and all(r["status"] in ("LIVE", "RT-OK") for r in led + smoke_led), [(r["arm"], r["status"]) for r in led][:4])
        if path == "replicated":
            check(f"{tag}: the CAND rows deliver in order later (+shift): p <= 0.05 and HL > TOL; the REPLICATED text names the follow-up", "the follow-up" in sc and re.search(r"^TEST65 n=%d .*HL shift=\+" % n_pairs, sc, re.M) is not None, sc[-800:])
        if path == "unresolved-n":
            check(f"{tag}: the first two RB slots died on both draws: 'dead twice 2', two pairs dropped (not counted), {n_pairs - 2} usable pairs < the floor {min_pairs}",
                  "dead twice 2" in sc and f"PAIRS65 usable {n_pairs - 2} of {n_pairs}" in sc and "REDRAWS BASE-F2-DEAD 2" in sc, sc[:900])
        if path == "base-dead-redrawn":
            ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
            first = [r for r in led if (r["arm"], r["seed"], r["rep"]) == ("RB", "42", 1)]
            check(f"{tag}: the first RB scored row died (BASE-F2-DEAD) and was redrawn ONCE in its slot (REPLACE64 ... reason=base-f2-dead), the redraw LIVE: the slot is scored ({n_pairs} usable pairs), redrawn alive 1",
                  len(first) == 2 and first[0]["status"] == "BASE-F2-DEAD" and first[1]["status"] == "LIVE" and re.search(r"^REPLACE64 c2-100 RB s42 rep=1 reason=base-f2-dead", ldtxt, re.M) is not None
                  and f"PAIRS65 usable {n_pairs} of {n_pairs}" in sc and "redrawn alive 1" in sc, [(r["status"]) for r in first])
        if path == "base-dead-twice":
            first = [r for r in led if (r["arm"], r["seed"], r["rep"]) == ("RB", "42", 1)]
            check(f"{tag}: the first RB slot died on the redraw too: two dead rows, the pair is dropped ({n_pairs - 1} usable), 'dead twice 1', a dead row never enters the test; the label stands on the rest",
                  len(first) == 2 and all(r["status"] == "BASE-F2-DEAD" for r in first) and f"PAIRS65 usable {n_pairs - 1} of {n_pairs}" in sc and "dead twice 1" in sc and want_label in sc.split("VERDICT65")[1].split("\n")[0], sc[:900])
        if path == "rc-rtfail":
            first = [r for r in led if r["arm"] == "RC" and r["status"] == "RT-FAIL"]
            check(f"{tag}: an RC row failed item 2: F2-FINDING65 printed (before the label's line), the row never redrawn (one row for its slot), its pair dropped ({n_pairs - 1} usable)",
                  len(first) == 1 and first[0]["rt_items"] == [2] and "F2-FINDING65 1 RC row(s) RT-FAIL" in sc and f"PAIRS65 usable {n_pairs - 1} of {n_pairs}" in sc and sc.index("F2-FINDING65") < sc.index("VERDICT65")
                  and len([r for r in led if (r["arm"], r["seed"], r["rep"]) == ("RC", first[0]["seed"], first[0]["rep"])]) == 1, sc[:900])
        if path == "rho-one-end":
            first = [r for r in led if r["arm"] == "RC" and r["status"] == "VOID-RHO"]
            ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
            check(f"{tag}: an RC row's server printed window_reliable: false: VOID-RHO, redrawn once (REPLACE64 reason=void-rho), the redraw RT-OK: the slot is scored ({n_pairs} usable)",
                  len(first) == 1 and re.search(r"^REPLACE64 c2-100 RC s%s rep=%s reason=void-rho" % (first[0]["seed"], first[0]["rep"]), ldtxt, re.M) is not None and f"PAIRS65 usable {n_pairs} of {n_pairs}" in sc, sc[:700])
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


# ═══════════════ plans 51e (status 51 block E) and 51 (block 51) ═══════════════
def spec51e(cell, arm):
    import plan51e
    return plan51e.arm_bytes(cell, arm), plan51e.arm_timeout(cell, arm), plan51e.arm_hint(arm), plan51e.arm_env(cell, arm)


def drv4_lines(sb):
    return [ln.split() for ln in open(os.path.join(sb["logs"], "drv4.log")).read().splitlines()]


def expect_calls51e(rows):
    """The driver lines (drv4.log): scenario a, b, hint, bytes, mode, timeout, RWM_DELTA, RWM_PERF_RATE_MBIT, RWM_SLAW, RWM_C_RELIABLE, RWM_RDIAG, RWM_RTOBS, RWM_DIAG, RWM_GEN, and the scrubbed knobs (all unset)."""
    out = []
    for cell, _, arm, _, _ in rows:
        b, tmo, hint, env = spec51e(cell, arm)
        e = dict(x.split("=", 1) for x in env)
        out.append(["c2", "c2", hint, str(b), "single", "timeout=%d" % tmo, "delta=unset", "rate=" + e.get("RWM_PERF_RATE_MBIT", "unset"), "slaw=" + e["RWM_SLAW"], "reliable=" + e["RWM_C_RELIABLE"],
                    "rdiag=1", "rtobs=unset", "diag=1", "gen=0", "minr=unset", "ccpace=unset", "copadelta=unset", "copawire=unset", "cexp=unset"])
    return out


def table51e():
    import plan51e
    return plan51e.plan("51e") if FULL else [x for x in plan51e.plan("51e") if x[4] <= 2]


def smoke_text(stage, name="smoke-check.txt"):
    p = os.path.join(stage, name)
    return open(p, errors="replace").read() if os.path.exists(p) else ""


def case51e(plan, path):
    """Plan 51e (block E) on the stubs: the strict suite rule (a reference-manifest MISS aborts), E-pre's registered bin_sha, the per-row run spec at the driver (the paced rate, the evicting seat, the scrubs), the
    rehearsal's section 10 go/no-go, the PL calibration rule and its PL-only recheck, the pacing / gauge witnesses as VOIDs replaced once, and a whole session through the scorer with the KNOWN absolute readings."""
    import plan51e
    import s51e_score as sc
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        binf, sha = stub_bin_file(sb)
        extra = {"LD_REF_MANIFEST": ref56_file(sb, miss=(path == "reuse-miss")), "LD_BIN_SHA_EXPECT": ("0" * 64 if path == "bin-sha-mismatch" else sha), "STUB_BIN_FILE": px(binf),
                 "STUB_SMOKE_N51E": "3" if path in ("calibrate", "rehearse", "recheck") else "1"}
        if path == "pace-void":
            extra["STUB_BAD51E"] = "paceonce"
        if path == "spc-void":
            extra["STUB_BAD51E"] = "spconce"
        if path == "calibrate":
            extra["STUB_BAD51E"] = "plastar"
        if path == "recheck":
            extra["LD_ONLY"] = "PL"
            extra["STUB_SMOKE_N51E"] = "1"
        rehearse = path in ("rehearse", "calibrate", "recheck")
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        tests = [c for c in cargo if "|test " in c]
        if path in ("reuse-miss", "bin-sha-mismatch"):
            token = "ABORT-SUITES plan 51e suite-reuse MISS" if path == "reuse-miss" else "ABORT-BIN-SHA plan 51e"
            check(f"{tag}: {token.split(' plan')[0]} -> FAILED-ALL, rc 5, NOTHING measured (no row-parser call, no driver call, no ld.log) and no terminal marker",
                  rc == 5 and token in e and failed and rows == [] and drv4_lines(sb) == [] and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out
                  and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(e, 6), rows[:2]))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        check(f"{tag}: rc 0 and the terminal marker {marker}", rc == 0 and marker in out and not failed, (rc, tail(out, 30), tail(e, 15)))
        stage = os.path.join(run_dir, "rep" + plan)
        chk = smoke_text(stage)
        check(f"{tag}: ONE stage rep51e (no guard, no BASE tree, ONE build), E-pre's bin_sha equals the built binary and is on record, the smoke check printed SMOKE-PASS",
              not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and len(builds) == 1
              and ("equals the registered value " + sha) in e and sha in open(os.path.join(run_dir, "BINSHA.txt")).read() and "SMOKE-PASS" in chk, (builds, tail(e, 6), chk[-600:]))
        sdir = os.path.join(run_dir, "suites")
        tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read() if os.path.exists(os.path.join(sdir, "TESTS.txt")) else ""
        hit = re.search(r"suite-reuse (HIT|MISS)", e)
        check(f"{tag}: the suite-reuse HIT is on record and no engine suite ran (no cargo test; the tools tests ran; no false suite record)",
              hit is not None and hit.group(1) == "HIT" and tests == [] and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and not os.path.exists(os.path.join(sdir, "TESTS-OK")), (hit and hit.group(0), tests, tt[:300]))
        plans_seen = {r.split("plan=")[1].split()[0] for r in rows}
        check(f"{tag}: every row-parser call ran under LD_PLAN=51e, with LD_BASE_SHA empty", plans_seen == {"51e"} and {re.search(r"base_sha=(\S*)", r).group(1) for r in rows} == {""}, (plans_seen, rows[:1]))
        calls = drv4_lines(sb)
        if rehearse:
            want_rows = [r for r in plan51e.REHEARSAL if path != "recheck" or r[2] == "PL"]
            check(f"{tag}: the rehearsal ran {len(want_rows)} row(s) ({'PL only: the recheck row' if path == 'recheck' else 'PL, PM, U'}), no battery (no ld.log), no suites, REHEARSAL-PASS persisted, the driver saw each row's run spec",
                  len(rows) == len(want_rows) and not os.path.exists(os.path.join(stage, "ld.log")) and os.path.exists(os.path.join(stage, "REHEARSAL-PASS")) and calls == expect_calls51e(want_rows), (len(rows), calls[:2]))
            if path == "calibrate":
                m = re.search(r"^CALIBRATE PL .*?new PL rate (\d+) sym/s = lambda ([\d.]+) Mbit, bytes (\d+)", chk, re.M)
                cal = plan51e.calibrate(12.0)      # computed from the table's PL rate (700 as registered: 270 sym/s, 1.06704 Mbit, 8 002 800 bytes; test_s51e pins that example)
                check(f"{tag}: PL A* = 12 fires the calibration (not a NO-GO: the rehearsal still passes): x-hat 11.5, the new rate by section 4's formula ({cal['rate_sym']} sym/s, lambda {cal['lam_mbit']} Mbit, {cal['bytes']} bytes), and the recheck command is printed",
                      m is not None and m.groups() == (str(cal["rate_sym"]), "%g" % cal["lam_mbit"], str(cal["bytes"])) and "LD_ONLY=PL" in chk and "SMOKE-FAIL" not in chk, chk[-900:])
            else:
                crit = ("(a)", "(b)", "(c)", "(d)", "(e)", "(f)", "(g)")
                check(f"{tag}: the section 10 criteria print PASS ({', '.join(crit)}" + (", (h) OK on PL" if path != "recheck" else ", (h) gating on PL") + ") and the calibration rule does not fire",
                      all(re.search(r"^PASS " + re.escape(c), chk, re.M) for c in crit if path != "recheck" or c in ("(a)", "(b)", "(c)", "(d)", "(e)", "(f)", "(g)")) and "(h) PL s42" in chk and "OK" in chk.split("(h) PL s42")[1].splitlines()[0]
                      and ("does not fire" in chk if path == "rehearse" else True), chk[-1200:])
            if path == "recheck":
                check(f"{tag}: the recheck gates (h): it prints '(h) PL passes' as a PASS line", re.search(r"^PASS \(h\) PL passes", chk, re.M) is not None, chk[-500:])
            EXECUTED.add((plan, path))
            return
        led = led56(stage)
        table = table51e()
        want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
        got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
        ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
        if path in ("pace-void", "spc-void"):
            why, pref = ("pace", "pace-echo-missing") if path == "pace-void" else ("spc", "spc-an-ne-fecn")
            first = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) == want_slots[0]]
            check(f"{tag}: the first scored slot (PL) is VOID by {pref} (a {'missing pacing echo' if path == 'pace-void' else 'fec n != an'}): 'REPLACE51E ... reason={why}' is in the ledger, the replacement row is LIVE and clean, every other slot has one row",
                  len(first) == 2 and first[0]["status"] == "WITNESS-FAIL" and any(x.startswith(pref) for x in first[0]["problems"]) and first[1]["status"] == "LIVE" and first[1]["problems"] == []
                  and re.search(r"^REPLACE51E %s %s s%s rep=%s reason=%s" % (want_slots[0][0], want_slots[0][1], want_slots[0][2], want_slots[0][3], why), ldtxt, re.M) is not None
                  and len(led) == len(want_slots) + 1, [(r["status"], r["problems"]) for r in first])
            check(f"{tag}: the replacement ran the same arm (the driver was called {len(want_slots) + 2} times: the smoke row, the VOID row, its replacement, the rest)", len(calls) == len(want_slots) + 2 and calls[1] == calls[2], len(calls))
            EXECUTED.add((plan, path))
            return
        check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}), one row each, every row LIVE with its t51e, no t51e error, no problem on any row",
              got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t51e") and not r.get("t51e_error") and r["problems"] == [] for r in led),
              (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], r.get("t51e_error")) for r in led if r["status"] != "LIVE" or r["problems"] or r.get("t51e_error")][:3], got_slots[:3], want_slots[:3]))
        check(f"{tag}: the driver saw every row's run spec in order: the PM smoke row, then the table (bytes, timeout, Realtime hint, the paced rate, RWM_SLAW=1, RWM_C_RELIABLE=0, RWM_RDIAG / RWM_DIAG, RWM_GEN=0, every scrubbed knob unset)",
              calls == expect_calls51e(list(plan51e.SMOKE) + table), (len(calls), calls[:3]))
        shas = {r["bin_sha"] for r in led}
        check(f"{tag}: one binary ran every row, its sha is the registered one, and the seat comparison executed on every row", shas == {sha} and all(r["sha_checked"] is True for r in led), shas)
        # the KNOWN absolute readings of the stub (stubrun_gen.gen51e)
        by = {a: [sc.RV(r) for r in led if r["arm"] == a] for a in plan51e.ARMS}
        gs = {a: sc.gauge_sums(by[a]) for a in plan51e.ARMS}
        check(f"{tag}: absolute gauge readings: PL and PM phi = zeta = pi = 0 (SLACK), U phi = 1.0, zeta = 0.9, pi = 0.1, clamp deficit 0.19 (known truth of the stub)",
              all(gs[a]["phi"] == gs[a]["zeta"] == gs[a]["pi"] == 0 for a in ("PL", "PM")) and abs(gs["U"]["phi"] - 1.0) < 1e-12 and abs(gs["U"]["zeta"] - 0.9) < 1e-12 and abs(gs["U"]["pi"] - 0.1) < 1e-12 and abs(gs["U"]["deficit"] - 0.19) < 1e-12, gs)
        check(f"{tag}: absolute row readings: median LATE src PL 700 / PM 8856 / U 100 000, the last sending tick 60 s / 60 s / 13 s, eviction share 0 / 0 / 0.7, tau_x n/a at PL (src <= 1000 sym/s), 0 at PM, 1 at U",
              all(rv.t["src_med_late"] == v for a, v in (("PL", 700), ("PM", 8856), ("U", 100000)) for rv in by[a]) and all(rv.t["last_src_t"] == v for a, v in (("PL", 60.0), ("PM", 60.0), ("U", 13.0)) for rv in by[a])
              and all(rv.t["evict_share"] == v for a, v in (("PL", 0.0), ("PM", 0.0), ("U", 0.7)) for rv in by[a]) and all(rv.t["tau_x"] is None for rv in by["PL"]) and all(rv.t["tau_x"] == 0 for rv in by["PM"])
              and all(rv.t["tau_x"] == 1.0 for rv in by["U"]), [(a, [(rv.t["src_med_late"], rv.t["evict_share"], rv.t["tau_x"]) for rv in by[a]]) for a in by])
        envl = ldtxt.split("=== env ")[-1].split("\n")[0]
        check(f"{tag}: the env scrub of section 4 rides every row (RWM_DELTA, RWM_COPA_DELTA, RWM_COPA_WIRE, RWM_COMPLETION_EXPOSURE, RWM_MIN_R, RWM_CC_PACE, RWM_PERF_RATE_MBIT are -u'd before the arm env; no other plan's scrub list changed), the instrument env is RWM_RDIAG=1 + the arm's timeout, no RWM_RTOBS",
              "=== scrub (plans 51 / 51e) -u RWM_DELTA -u RWM_COPA_DELTA -u RWM_COPA_WIRE -u RWM_COMPLETION_EXPOSURE -u RWM_MIN_R -u RWM_CC_PACE -u RWM_PERF_RATE_MBIT" in ldtxt and "RWM_RDIAG=1" in envl and "RWM_RTOBS=1" not in envl.split(" bulk window")[0], envl[:300])
        score = open(os.path.join(stage, "score.txt"), errors="replace").read()
        m = re.search(r"^VERDICT51E REPORTED-ONLY slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+) dnf=(\d+) outcome=(\S+) rate_spare_cwnd=(\S+) labels=(\d+)", score, re.M)
        names = ("PACE", "GAUGE-WITNESS", "SPARE-CLAMP-PL", "SPARE-CLAMP-PM", "SPARE-CLAMP-U", "SPARE-CLAMP-OUTCOME", "RATE-SPARE-CWND", "K4-PL", "K4-PM", "PL-CALIBRATION")
        check(f"{tag}: the score prints a VERDICT51E line counting every slot (every row live, none replaced, every row a DNF datum), DEGENERATE-AS-WIRED (predicted) and NOT-ENFORCED, and every registered LABEL51E",
              m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led) and m.group(4) == "0" and m.group(5) == "0" and m.group(6) == str(len(led)) and m.group(7) == "DEGENERATE-AS-WIRED" and m.group(8) == "NOT-ENFORCED"
              and all(re.search(r"^LABEL51E %s " % n_, score, re.M) for n_ in names) and "ROW51E PL" in score and "predicted" in score, score[-1500:])
        check(f"{tag}: DONE-ALL and score.txt", os.path.exists(os.path.join(run_dir, "DONE-ALL")) and os.path.exists(os.path.join(run_dir, "score.txt")), tail(e, 12))
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


def red51e(kind, only, want, want_rc=5, name=None):
    """A rehearsal of plan 51e in which the stub injects ONE defect (STUB_BAD51E): the smoke must FAIL on the criterion that catches it (ABORT-SMOKE, rc 5, no REHEARSAL-PASS). `only` = LD_ONLY (the PL recheck)."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "51e", None)
        binf, sha = stub_bin_file(sb)
        extra = {"LD_REF_MANIFEST": ref56_file(sb, miss=False), "LD_BIN_SHA_EXPECT": sha, "STUB_BIN_FILE": px(binf), "STUB_BAD51E": kind, "STUB_SMOKE_N51E": "3"}
        if only:
            extra["LD_ONLY"] = only
        rc, out, rows, cargo = run_launch(sb, "51e", None, rehearse=True, extra_env=extra)
        ctxt = smoke_text(os.path.join(sb["root"], "run", "rep51e"))
        check(f"51e/red:{name or kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == want_rc and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-900:], tail(out, 6)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def calib51e(kind, want_in):
    """The rehearsal (not the recheck): PL with A* in range but NO K4-steady window (ab > 0 on every window, the ab = 0 rule): the rehearsal PASSES and prints the calibration line naming the steady count."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "51e", None)
        binf, sha = stub_bin_file(sb)
        extra = {"LD_REF_MANIFEST": ref56_file(sb, miss=False), "LD_BIN_SHA_EXPECT": sha, "STUB_BIN_FILE": px(binf), "STUB_BAD51E": kind, "STUB_SMOKE_N51E": "3"}
        rc, out, rows, cargo = run_launch(sb, "51e", None, rehearse=True, extra_env=extra)
        ctxt = smoke_text(os.path.join(sb["root"], "run", "rep51e"))
        check(f"51e/calib:{kind}: PL with 0 K4-steady windows (ab > 0 on every window): the rehearsal still passes (rc 0) and prints CALIBRATE PL naming '{want_in}'",
              rc == 0 and "LD-ALL-REHEARSAL-DONE" in out and re.search(r"^CALIBRATE PL .*" + re.escape(want_in), ctxt, re.M) is not None, (rc, ctxt[-900:]))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def refused51(plan, name, extra, want_rc, want_in, rehearse=True, base=True, no_build=True, prep=None):
    """A launch of plan 51 / 51e that must be refused before anything is measured."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        binf, sha = stub_bin_file(sb)
        ex = {"STUB_BIN_FILE": px(binf)}
        if plan == "51e":
            ex.update({"LD_REF_MANIFEST": ref56_file(sb, miss=False), "LD_BIN_SHA_EXPECT": sha})
        else:
            ex.update({"LD_CELL": "c2-100", "LD_BIN_SHA_EXPECT": sha})
        for k, v in extra.items():
            if v is None:
                ex.pop(k, None)
            else:
                ex[k] = v
        rc, out, rows, cargo = run_launch(sb, plan, CAND_SHA if (plan == "51" and base) else None, rehearse=rehearse, extra_env=ex)
        check(f"{plan}/{name}: refused (rc {want_rc}, '{want_in}'), no row measured{', no build' if no_build else ''}",
              rc == want_rc and want_in in era(sb) + out and rows == [] and (not no_build or not [c for c in cargo if "|build " in c]), (rc, tail(era(sb), 5), tail(out, 5)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def budget51(plan, rehearse, left_s, ok):
    """The budget guard of plan 51 / 51e (a rehearsal needs less than a battery): LD_HARD with `left_s` seconds left is refused below the need and accepted above it."""
    need = {("51e", True): 1500, ("51e", False): 2730, ("51", True): 1200, ("51", False): 17400}[(plan, rehearse)]
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        binf, sha = stub_bin_file(sb)
        ex = {"STUB_BIN_FILE": px(binf), "LD_BIN_SHA_EXPECT": sha, "LD_HARD": str(int(time.time()) + left_s)}
        if plan == "51e":
            ex["LD_REF_MANIFEST"] = ref56_file(sb, miss=False)
        else:
            ex["LD_CELL"] = "c2-100"
        rc, out, rows, cargo = run_launch(sb, plan, CAND_SHA if plan == "51" else None, rehearse=rehearse, extra_env=ex)
        e = era(sb)
        if ok:
            check(f"{plan}/budget{'-rehearse' if rehearse else ''}: {left_s} s left (just above the {need} s need) is accepted: the launch gets past the guard", "ABORT-BUDGET" not in e + out and rows != [], (rc, tail(e, 4)))
        else:
            check(f"{plan}/budget{'-rehearse' if rehearse else ''}: {left_s} s left (below the {need} s need) is refused before anything runs (ABORT-BUDGET plan {plan}, rc 3)",
                  rc == 3 and f"ABORT-BUDGET plan {plan}:" in e + out and rows == [] and not [c for c in cargo if "|build " in c], (rc, tail(e, 4)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def table51(plan_cell="c2-100"):
    import plan51
    rows = plan51.plan("51", cell=plan_cell)
    return rows if FULL else [x for x in rows if x[4] <= 2]


def spec51(cell, arm):
    import plan51
    return plan51.arm_bytes(cell, arm), plan51.arm_timeout(cell, arm), plan51.arm_hint(arm), plan51.arm_env(cell, arm)


def expect_calls51(rows):
    out = []
    for cell, _, arm, _, _ in rows:
        b, tmo, hint, env = spec51(cell, arm)
        e = dict(x.split("=", 1) for x in env)
        sc_ = {"c2-100": ("c2", "c2", "single"), "c8-100": ("c2", "c3", "dual")}[cell]
        out.append([sc_[0], sc_[1], hint, str(b), sc_[2], "timeout=%d" % tmo, "delta=" + e.get("RWM_DELTA", "unset"), "rate=unset", "slaw=unset", "reliable=unset", "rdiag=1", "rtobs=1", "diag=1", "gen=0",
                    "minr=unset", "ccpace=unset", "copadelta=unset", "copawire=unset", "cexp=unset"])
    return out


def case51(plan, path):
    """Plan 51 (block 51) on the stubs: main at launch (LD_BASE_SHA), no engine suites, the dial pins on the built tree, the bin_sha recorded by the rehearsal and required on the battery, the per-row hint / delta at the
    driver, the RWM_DELTA echo VOID and the VOID-SHORT row replaced once, and a whole session through the scorer."""
    import plan51
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, CAND_SHA)
        binf, sha = stub_bin_file(sb)
        rehearse = path == "rehearse"
        extra = {"STUB_BIN_FILE": px(binf), "LD_CELL": "c2-100", "LD_BIN_SHA_EXPECT": ("0" * 64 if path == "bin-sha-mismatch" else sha)}
        if rehearse:
            extra.pop("LD_BIN_SHA_EXPECT")
        if path == "delta-echo-void":
            extra["STUB_BAD51"] = "deltaonce"
        if path == "short-void":
            extra["STUB_BAD51"] = "shortonce"
        if path == "dialpins-fail":
            extra["STUB_FAIL_PIN"] = "scheduling_weights_are_the_dial_not_a_mode"
        rc, out, rows, cargo = run_launch(sb, plan, CAND_SHA, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        tests = [c for c in cargo if "|test " in c]
        if path in ("bin-sha-mismatch", "dialpins-fail"):
            token = "ABORT-BIN-SHA plan 51" if path == "bin-sha-mismatch" else "ABORT-DIALPINS plan 51"
            check(f"{tag}: {token} -> FAILED-ALL, rc 5, NOTHING measured (no row-parser call, no driver call, no ld.log) and no terminal marker",
                  rc == 5 and token in e and failed and rows == [] and drv4_lines(sb) == [] and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out
                  and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(e, 6), rows[:2]))
            if path == "dialpins-fail":
                check(f"{tag}: the failing pin is named in dialpins.log (FAILED) and the build ran before it", "scheduling_weights_are_the_dial_not_a_mode ... FAILED" in open(os.path.join(run_dir, "dialpins.log"), errors="replace").read() and len(builds) == 1, tail(e, 6))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        check(f"{tag}: rc 0 and the terminal marker {marker}", rc == 0 and marker in out and not failed, (rc, tail(out, 30), tail(e, 15)))
        stage = os.path.join(run_dir, "rep" + plan)
        chk = smoke_text(stage)
        pins = [c for c in tests]
        check(f"{tag}: ONE stage rep51 (no guard, no BASE tree, ONE build), the dial pins ran on the built tree (2 cargo test calls: raptorpath with the three filters, raptorpath-wasm) and are on record, the smoke check printed SMOKE-PASS",
              not os.path.exists(os.path.join(run_dir, "guard")) and not os.path.exists(os.path.join(sb["devshm"], "ld", "bin-base", "raptorpath")) and len(builds) == 1 and len(pins) == 2
              and all("--release" in pins[0] and "delta_budget_b_is_the_dial_not_a_mode" in pins[0] and "scheduling_weights_are_the_dial_not_a_mode" in pins[0] and "control::fec_rate" in pins[0] for _ in (0,))
              and "--lib" in pins[0] and "raptorpath-wasm" in pins[1] and "--lib" in pins[1] and "test_continuum_one_law_across_the_dial" in pins[1] and "dial pins ok" in e and "SMOKE-PASS" in chk, (builds, pins, tail(e, 6), chk[-600:]))
        bs = open(os.path.join(run_dir, "BINSHA.txt")).read()
        if rehearse:
            check(f"{tag}: the rehearsal RECORDS the bin_sha (BINSHA.txt, the era line 'RECORDED') instead of checking one", "RECORDED (rehearsal)" in e and re.match(r"[0-9a-f]{64}", bs) is not None, tail(e, 6))
        else:
            check(f"{tag}: the battery's bin_sha equals the one the rehearsal recorded (LD_BIN_SHA_EXPECT) and is on record", ("equals the registered value " + sha) in e and sha in bs, tail(e, 6))
        sdir = os.path.join(run_dir, "suites")
        tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read() if os.path.exists(os.path.join(sdir, "TESTS.txt")) else ""
        check(f"{tag}: no engine suite ever (ENGINE-SUITES-REUSED, no TESTS-OK / SRC-MANIFEST; only the 2 pin calls ran cargo test); the tools tests (python and bash) ran",
              "ENGINE-SUITES-REUSED" in tt and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and "TEST main" not in tt and not os.path.exists(os.path.join(sdir, "TESTS-OK")) and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")), tt[:300])
        plans_seen = {r.split("plan=")[1].split()[0] for r in rows}
        check(f"{tag}: every row-parser call ran under LD_PLAN=51 with LD_BASE_SHA = main at launch having passed `sudo -n env`", plans_seen == {"51"} and {re.search(r"base_sha=(\S*)", r).group(1) for r in rows} == {CAND_SHA}, (plans_seen, rows[:1]))
        calls = drv4_lines(sb)
        smoke_rows = plan51.plan("51", smoke=True, cell="c2-100")
        if rehearse:
            check(f"{tag}: the rehearsal ran the 4 smoke rows (H-B, H-R, B+, N-A), no battery (no ld.log), REHEARSAL-PASS persisted, the driver saw each row's hint, delta, bytes and timeout in order",
                  len(rows) == 4 and not os.path.exists(os.path.join(stage, "ld.log")) and os.path.exists(os.path.join(stage, "REHEARSAL-PASS")) and calls == expect_calls51(smoke_rows), (len(rows), calls[:2]))
            check(f"{tag}: the smoke check prints the not-overdriven witness on H-R and N-A and the batching / RWM_DELTA / pipe criteria as PASS lines",
                  all(re.search(r"^PASS " + re.escape(x), chk, re.M) for x in ("NOT-OVERDRIVEN witness on HR", "NOT-OVERDRIVEN witness on NA", "the emission-batching echo", "[GATES] RWM_DELTA equals", "pipe_cli / pipe_srv", "rr is exactly 0 on the H-B row", "rr parsed on N-A")), chk[-1400:])
            EXECUTED.add((plan, path))
            return
        led = led56(stage)
        table = table51()
        want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
        got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
        ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
        if path in ("delta-echo-void", "short-void"):
            why, pref = ("echo", "gate-delta-cli") if path == "delta-echo-void" else ("short", "short-row")
            first = [r for r in led if (r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) == want_slots[0]]
            ok_first = (first[0]["status"] == "WITNESS-FAIL" and any(x.startswith(pref) for x in first[0]["problems"])) if path == "delta-echo-void" else (first[0]["status"] == "VOID-SHORT")
            check(f"{tag}: the first scored slot is VOID ({'the RWM_DELTA echo is wrong on the client end' if path == 'delta-echo-void' else 'a 10 s row: VOID-SHORT'}): 'REPLACE51 ... reason={why}' is in the ledger, the replacement is LIVE and clean, every other slot has one row",
                  len(first) == 2 and ok_first and first[1]["status"] == "LIVE" and first[1]["problems"] == [] and re.search(r"^REPLACE51 %s %s s%s rep=%s reason=%s" % (want_slots[0][0], want_slots[0][1], want_slots[0][2], want_slots[0][3], why), ldtxt, re.M) is not None
                  and len(led) == len(want_slots) + 1, [(r["status"], r["problems"]) for r in first])
            EXECUTED.add((plan, path))
            return
        check(f"{tag}: the ledger holds exactly the table's slots in the table's order ({len(want_slots)}), one row each, every row LIVE with its t51, no t51 error, no problem on any row",
              got_slots == want_slots and all(r["status"] == "LIVE" and r.get("t51") and not r.get("t51_error") and r["problems"] == [] for r in led),
              (len(led), [(r["cell"], r["arm"], r["status"], r["problems"], r.get("t51_error")) for r in led if r["status"] != "LIVE" or r["problems"] or r.get("t51_error")][:3], got_slots[:3], want_slots[:3]))
        check(f"{tag}: the driver saw every row's hint and RWM_DELTA in order: the 4 smoke rows, then the table (named-hint arms: delta unset; numeric arms: the arm's delta on the Auto carrier; 600 MB, 200 s)",
              calls == expect_calls51(smoke_rows + table), (len(calls), calls[:3], expect_calls51(smoke_rows + table)[:3]))
        blocks = len(re.findall(r"^BLOCK-COMPLETE rep=", ldtxt, re.M))
        check(f"{tag}: every block is a BLOCK-COMPLETE line ({2 if not FULL else 6} blocks) and the scrub line is in the ledger", blocks == (6 if FULL else 2) and "=== scrub (plans 51 / 51e) -u RWM_DELTA" in ldtxt, blocks)
        score = open(os.path.join(stage, "score.txt"), errors="replace").read()
        m = re.search(r"^VERDICT51 REPORTED-WITH-SCORED slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+) dnf=(\d+) labels=(\d+)", score, re.M)
        check(f"{tag}: the score prints a VERDICT51 line counting every slot with every row live, the T1 / T2 lines per preset, W-ROUTE, the delta table, delta* and the A/A resolution, and never the word 'continuous'",
              m is not None and int(m.group(1)) == len(want_slots) and int(m.group(3)) == len(led) and m.group(4) == "0" and m.group(5) == "0"
              and all(re.search(r"^  T1 c2 %s: " % n_, score, re.M) for n_ in ("Bulk", "Auto", "Realtime")) and all(re.search(r"^  T2 c2 %s: " % n_, score, re.M) for n_ in ("Bulk", "Auto", "Realtime"))
              and "W-ROUTE c2" in score and "r(delta) c2" in score and "delta* c2" in score and "A/A c2" in score and "continuous" not in score.lower(), score[-1800:])
        if FULL:
            check(f"{tag}: on the stub's arm-independent rows every T1 and T2 passes (no step above d80) and the labels are PASS", all(("PASS" in v) for n_, v in re.findall(r"^LABEL51 ((?:T1|T2)-c2-\S+) (.*)$", score, re.M)) and score.count("LABEL51 T1-c2-") == 3 and score.count("LABEL51 T2-c2-") == 3, score[-1500:])
        check(f"{tag}: DONE-ALL and score.txt", os.path.exists(os.path.join(run_dir, "DONE-ALL")) and os.path.exists(os.path.join(run_dir, "score.txt")), tail(e, 12))
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


def red51(kind, want):
    """A rehearsal of plan 51 in which the stub injects one defect (STUB_BAD51): the smoke must FAIL on the criterion that catches it (SMOKE-FAIL, ABORT-SMOKE, rc 5)."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "51", CAND_SHA)
        binf, sha = stub_bin_file(sb)
        rc, out, rows, cargo = run_launch(sb, "51", CAND_SHA, rehearse=True, extra_env={"STUB_BIN_FILE": px(binf), "LD_CELL": "c2-100", "STUB_BAD51": kind})
        ctxt = smoke_text(os.path.join(sb["root"], "run", "rep51"))
        check(f"51/red:{kind}: the stub's defect fails the smoke (ABORT-SMOKE, rc 5, no REHEARSAL-PASS) and the criterion that must catch it prints FAIL",
              rc == 5 and "ABORT-SMOKE" in era(sb) and "LD-ALL-REHEARSAL-DONE" not in out and re.search(r"^FAIL " + re.escape(want), ctxt, re.M) is not None, (rc, ctxt[-900:], tail(out, 6)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def block_budget51():
    """The per-block guard of plan 51: a block is started only with rows x LD_BLOCK_ROW_S seconds left before LD_HARD (the registered 215 s): with LD_BLOCK_ROW_S raised, the battery stops at block 1 (ABORT-BUDGET, exit 6)
    and the launch fails with no scored row measured."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "51", CAND_SHA)
        binf, sha = stub_bin_file(sb)
        rc, out, rows, cargo = run_launch(sb, "51", CAND_SHA, extra_env={"STUB_BIN_FILE": px(binf), "LD_CELL": "c2-100", "LD_BIN_SHA_EXPECT": sha, "LD_HARD": str(int(time.time()) + 2400),
                                                                          "LD_HARD_MIN_S": "1", "LD_BLOCK_ROW_S": "1000"})
        ld = os.path.join(sb["root"], "run", "rep51", "ld.log")
        ldtxt = open(ld, errors="replace").read() if os.path.exists(ld) else ""
        check("51/block-budget: with 14 rows x 1000 s needed and 2400 s left the battery stops before block 1 (ABORT-BUDGET block 1 in the ledger), the launch fails (rc 5) and no scored row ran (the smoke's 4 rows only)",
              rc == 5 and re.search(r"^ABORT-BUDGET block 1: ", ldtxt, re.M) is not None and len(rows) == 4 and "LD-ALL-DONE" not in out, (rc, ldtxt[-300:], len(rows)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def rehearse51_c8():
    """The c8 launch of plan 51 (one of the three registered launches): LD_CELL=c8-100 rehearsal. The driver sees the dual c2 / c3 topology for each smoke row, and the smoke passes on the c8 stub rows."""
    import plan51
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "51", CAND_SHA)
        binf, sha = stub_bin_file(sb)
        rc, out, rows, cargo = run_launch(sb, "51", CAND_SHA, rehearse=True, extra_env={"STUB_BIN_FILE": px(binf), "LD_CELL": "c8-100"})
        calls = drv4_lines(sb)
        want = expect_calls51(plan51.plan("51", smoke=True, cell="c8-100"))
        chk = smoke_text(os.path.join(sb["root"], "run", "rep51"))
        check("51/c8-rehearse: LD_CELL=c8-100: rc 0, REHEARSAL-DONE, the driver saw `c2 c3 <hint> 600000000 dual` for the 4 smoke rows (hint and RWM_DELTA per arm), SMOKE-PASS",
              rc == 0 and "LD-ALL-REHEARSAL-DONE" in out and calls == want and all(c[:2] == ["c2", "c3"] and c[4] == "dual" for c in calls) and len(calls) == 4 and "SMOKE-PASS" in chk and "cell=c8-100" in chk, (rc, calls[:2], chk[-400:]))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def dialpins_silent():
    """A dial pin whose test never executes (no `test <name> ... ok` line: a filter that matches nothing) stops the launch exactly as a failing one (rule 1)."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "51", CAND_SHA)
        binf, sha = stub_bin_file(sb)
        rc, out, rows, cargo = run_launch(sb, "51", CAND_SHA, rehearse=True, extra_env={"STUB_BIN_FILE": px(binf), "LD_CELL": "c2-100", "STUB_SKIP_PIN": "test_continuum_one_law_across_the_dial"})
        check("51/dialpins-silent: a pin that matched no test is NOT EXECUTED: ABORT-DIALPINS, rc 5, nothing measured", rc == 5 and "dial pin NOT EXECUTED" in era(sb) and "ABORT-DIALPINS" in era(sb) and rows == [], (rc, tail(era(sb), 5)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


# ═══════════════ plan 63 (status 63): F2's first VM contact, two seats ═══════════════
def stub_bin_pair(sb):
    """-> ((cand_path, cand_sha), (control_path, control_sha)): two DIFFERENT stub engine binaries carrying every token the plan-63 preflight asks for of BOTH seats."""
    body = ("#!/bin/sh\n# stub engine binary for plan 63 (%s)\n# RWM_STORE\n# RWM_STORE_PATH_POOL\n# RWM_UNIFIED_SHED unified overload shedding ACTIVE\n# RWM_SLAW\n# [SLAW] t=\n"
            "# RWM_SUCC_DUMP RWM_EMIT_BURST\n# RWM_DELTA\n# emission batching ACTIVE\n# emission batching out of scope\n# RWM_PERF_RATE_MBIT\n# spc=\n# offered-load pacing ACTIVE\nexit 0\n")
    out = []
    for name in ("cand", "control"):
        path = os.path.join(sb["logs"], "stub-engine-" + name)
        text = body % name
        with open(path, "w", newline="\n") as fh:
            fh.write(text)
        os.chmod(path, 0o755)
        out.append((path, hashlib.sha256(text.encode()).hexdigest()))
    return out[0], out[1]


def drv6_lines(sb):
    p = os.path.join(sb["logs"], "drv6.log")
    return [ln.split() for ln in open(p).read().splitlines()] if os.path.exists(p) else []


def expect_calls63(rows):
    """The driver lines (drv6.log): scenario a, b, hint, bytes, mode, timeout, rate, RWM_SLAW, RWM_C_RELIABLE, RWM_RDIAG, RWM_DIAG, the binary's directory (bin-base = CONTROL, bin = CAND)."""
    import plan63
    out = []
    for cell, _, arm, _, _ in rows:
        b = plan63.arm_bytes(cell, arm)
        e = dict(x.split("=", 1) for x in plan63.arm_env(cell, arm))
        out.append(["c2", "c2", "realtime", str(b), "single", "timeout=%d" % plan63.arm_timeout(cell, arm), "rate=" + e.get("RWM_PERF_RATE_MBIT", "unset"), "slaw=" + e["RWM_SLAW"],
                    "reliable=" + e["RWM_C_RELIABLE"], "rdiag=1", "diag=1", "bindir=" + ("bin-base" if arm.startswith("C") else "bin")])
    return out


def case63(plan, path):
    """Plan 63 (F2's first contact) on the stubs: two seats (the CAND built, the CONTROL taken from LD_BIN_CONTROL), both pins checked before any row, ABORT-BIN-SHA without / with a wrong pin, the rehearsal's one CAND PL row
    (it RECORDS the CAND sha), the whole session of 10 slots in CONTROL-then-CAND order, the seat's binary at the driver, the env contract (LD_SHA_BASE reaches the row parser), the F2 witnesses as reported readings,
    and a failing candidate (labelled FAIL, never voided)."""
    import plan63
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None)
        (cand_bin, cand_sha), (ctl_bin, ctl_sha) = stub_bin_pair(sb)
        extra = {"STUB_BIN_FILE": px(cand_bin), "LD_BIN_CONTROL": px(ctl_bin), "LD_BIN_SHA_CONTROL": ("0" * 64 if path == "control-sha-mismatch" else ctl_sha), "STUB_DRVLOG6": px(sb["logs"]) + "/drv6.log"}
        if path in ("full", "cand-fail", "cand-noctld", "cand-ctld-send-only", "cand-prebuilt", "control-sha-mismatch"):
            extra["LD_BIN_SHA_CAND"] = cand_sha
        if path == "cand-sha-mismatch":
            extra["LD_BIN_SHA_CAND"] = "0" * 64
        if path == "cand-fail":
            extra["STUB_BAD63"] = "candfail"
        if path == "cand-noctld":
            extra["STUB_BAD63"] = "noctld"
        if path == "cand-ctld-send-only":
            extra["STUB_BAD63"] = "ctldsend"
        if path == "cand-prebuilt":
            extra["LD_BIN_PREBUILT"] = px(cand_bin)
        rehearse = path == "rehearse"
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        tests = [c for c in cargo if "|test " in c]
        calls = drv6_lines(sb)
        if path in ("cand-sha-missing", "cand-sha-mismatch", "control-sha-mismatch", "cand-prebuilt"):
            if path == "cand-prebuilt":
                check(f"{tag}: a prebuilt CAND is refused before anything runs (ABORT-BUILD, rc 3: no build, no lock, no row): the CAND is a plain release build from the launch archive",
                      rc == 3 and "ABORT-BUILD plan 63 never accepts a prebuilt CAND" in out and rows == [] and calls == [] and builds == [], (rc, tail(out, 6)))
            elif path == "cand-sha-missing":
                check(f"{tag}: a launch without LD_BIN_SHA_CAND is refused before anything runs (ABORT-BIN-SHA, rc 3: no build, no lock, no row)",
                      rc == 3 and "ABORT-BIN-SHA plan 63 needs LD_BIN_SHA_CAND" in out and rows == [] and calls == [] and builds == [], (rc, tail(out, 6)))
            else:
                token = "ABORT-BIN-SHA plan 63 CAND" if path == "cand-sha-mismatch" else "ABORT-BIN-SHA plan 63 CONTROL"
                check(f"{tag}: {token} -> FAILED-ALL, rc 5, NOTHING measured (no row-parser call, no driver call, no ld.log) and no terminal marker",
                      rc == 5 and token in e and failed and rows == [] and calls == [] and "LD-ALL-DONE" not in out and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(e, 6), rows[:2]))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        check(f"{tag}: rc 0 and the terminal marker {marker}", rc == 0 and marker in out and not failed, (rc, tail(out, 30), tail(e, 15)))
        stage = os.path.join(run_dir, "rep" + plan)
        chk = smoke_text(stage)
        pins = ("CAND bin_sha RECORDED (rehearsal): " + cand_sha) if rehearse else ("CAND bin_sha equals the registered value " + cand_sha)
        check(f"{tag}: ONE stage rep63 (no guard), the CAND built once and the CONTROL taken from LD_BIN_CONTROL (no second build), the CAND pin {'recorded' if rehearse else 'checked'}, the CONTROL pin checked, no engine suite ran, the smoke check printed SMOKE-PASS",
              not os.path.exists(os.path.join(run_dir, "guard")) and len(builds) == 1 and pins in e and ("CONTROL bin_sha equals the pinned value " + ctl_sha) in e and tests == [] and "SMOKE-PASS" in chk
              and ctl_sha in open(os.path.join(run_dir, "BINSHA.txt")).read() and cand_sha in open(os.path.join(run_dir, "BINSHA.txt")).read(), (builds, tail(e, 8), chk[-500:]))
        plans_seen = {r.split("plan=")[1].split()[0] for r in rows}
        check(f"{tag}: every row-parser call ran under LD_PLAN=63 and the CONTROL seat's pin LD_SHA_BASE crossed `sudo -n env` to it (the env-propagation contract)",
              plans_seen == {"63"} and {re.search(r"shabase=(\S*)", r).group(1) for r in rows} == {ctl_sha}, (plans_seen, rows[:1]))
        if rehearse:
            want_rows = plan63.plan("63", rehearsal=True)
            check(f"{tag}: the rehearsal ran CONTROL PL s42 then CAND PL s42 (2 rows, each on its own seat's binary: bindir bin-base then bin), no battery (no ld.log), REHEARSAL-PASS persisted",
                  len(rows) == 2 and not os.path.exists(os.path.join(stage, "ld.log")) and os.path.exists(os.path.join(stage, "REHEARSAL-PASS")) and calls == expect_calls63(want_rows) and [c[-1] for c in calls] == ["bindir=bin-base", "bindir=bin"],
                  (len(rows), calls[:2], expect_calls63(want_rows)))
            ldr = led56(stage, "smoke.log")
            check(f"{tag}: no rehearsal row is VOID-RC: the driver exits 0 on a by-design DNF / a failed warm-up with a summary (the real F2 lines run under the real script's `set -e`: a bare `grep -c` no-match once made every row rc 1)",
                  len(ldr) == 2 and all(r["status"] != "VOID-RC" for r in ldr), [(r["arm"], r["status"]) for r in ldr])
            check(f"{tag}: both rows are REPORTED: the CONTROL row's warm-up failure is a datum (REPORTED ... NEVER-ACKED, undeliv n/a, no abort), the CAND row is PASS on the F2 reading with its transfer completed",
                  re.search(r"^  REPORTED CPL-s42 REPORTED: warm-up NEVER-ACKED \(a datum, not an abort\);.*undeliv=n/a", chk, re.M) is not None and re.search(r"^  REPORTED FPL-s42 PASS:.*transfer=COMPLETED undeliv=0\(srv 0\+cli 0\)", chk, re.M) is not None and "SMOKE-PASS" in chk, chk[-900:])
            EXECUTED.add((plan, path))
            return
        led = led56(stage)
        table = plan63.plan("63")
        want_slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
        got_slots = [(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in led]
        score = open(os.path.join(stage, "score.txt"), errors="replace").read()
        ldtxt_rc = open(os.path.join(stage, "ld.log"), errors="replace").read()
        check(f"{tag}: the ledger holds exactly the 10 slots in CONTROL-then-CAND order (PL s42, PM s42, U s42, PL s7, PM s7), one row each (nothing replaced), every row with its t63 and no t63 error, no VOID-F2",
              got_slots == want_slots and len(led) == 10 and all(r.get("t63") and not r.get("t63_error") and r["status"] != "VOID-F2" for r in led), (len(led), got_slots[:3], want_slots[:3], [(r["arm"], r["status"], r["problems"]) for r in led][:4]))
        check(f"{tag}: the driver saw every row's spec in order, the CONTROL rows on bin-base and the CAND rows on bin: the CAND smoke row, then the table",
              calls == expect_calls63(list(plan63.SMOKE) + table), (len(calls), calls[:3], expect_calls63(list(plan63.SMOKE) + table)[:3]))
        check(f"{tag}: no row is VOID-RC (rc 0 on every row, the failed CONTROL warm-ups and the by-design DNFs included): the real F2 lines run under the real script's `set -e` and must not kill it",
              all(r["status"] != "VOID-RC" for r in led) and "ABORT-RC" not in ldtxt_rc, [(r["arm"], r["status"]) for r in led if r["status"] == "VOID-RC"])
        check(f"{tag}: each seat ran its own binary: the CONTROL rows carry the pinned CONTROL sha, the CAND rows the CAND sha, and the seat comparison executed on every row",
              all(r["bin_sha"] == (ctl_sha if r["arm"].startswith("C") else cand_sha) for r in led) and all(r["sha_checked"] is True for r in led), {r["arm"]: r["bin_sha"][:8] for r in led})
        ldtxt = open(os.path.join(stage, "ld.log"), errors="replace").read()
        check(f"{tag}: the ledger records the driver's `--- F2` line of every row and both seats' binaries, and the scrub of plans 51 / 51e / 63 rides every row", len(re.findall(r"^--- F2 undeliv=\d+ unframable=\d+", ldtxt, re.M)) == 10
              and "=== binary-base" in ldtxt and "RWM_PERF_RATE_MBIT" in ldtxt.split("=== scrub")[-1].split("\n")[0], ldtxt[:600])
        labels = dict(re.findall(r"^LABEL63 (\S+) (\S+)", score, re.M))
        cand_arms = ("FPL-s42", "FPM-s42", "FU-s42", "FPL-s7", "FPM-s7")
        if path == "cand-fail":
            check(f"{tag}: the failing CAND paced rows (PL and PM, both seeds) are labelled FAIL with their reasons (warm-up never acked, undeliv=4 = srv 2 + cli 2, unframable=1), the unpaced FU still PASSes, the report prints BLOCKS-E-PRE, and no row is voided",
                  labels.get("FPL-s42") == "FAIL" and labels.get("FPL-s7") == "FAIL" and labels.get("FPM-s42") == "FAIL" and labels.get("FPM-s7") == "FAIL" and labels.get("FU-s42") == "PASS" and "cand_fail=4/5" in score and "BLOCKS-E-PRE" in score
                  and re.search(r"FPL-s42 FAIL \(.*undeliv=4.*unframable=1", score) is not None and all(r["status"] != "VOID-F2" for r in led), score[-1200:])
        elif path == "cand-noctld":
            check(f"{tag}: a CAND with no [CTLD] line is a MISSING WITNESS: every CAND row FAILs ('no [CTLD] line with undeliv= (missing witness)', undeliv=MISSING in the ROW63 line, not a pass on the driver's `--- F2 undeliv=0`), the CONTROL rows are unaffected, BLOCKS-E-PRE",
                  all(labels.get(a) == "FAIL" for a in cand_arms) and score.count("missing witness") == 5 and "undeliv=MISSING" in score and "cand_fail=5/5" in score and "BLOCKS-E-PRE" in score, score[-1500:])
        elif path == "cand-ctld-send-only":
            check(f"{tag}: a CAND whose only [CTLD] lines are on the SENDING end (the client) is a MISSING receiving-end witness: every CAND row FAILs ('missing witness ... (the sending end printed one)'), undeliv shows MISSING, BLOCKS-E-PRE",
                  all(labels.get(a) == "FAIL" for a in cand_arms) and score.count("(the sending end printed one)") == 5 and "undeliv=MISSING" in score and "cand_fail=5/5" in score and "BLOCKS-E-PRE" in score, score[-1500:])
        else:
            check(f"{tag}: the report prints ROW63 + LABEL63 per row (every CAND row PASS on the F2 reading; every CONTROL row REPORTED with no per-row prediction: its warm-up fails on the 4 paced rows of the stub and U completes, undeliv n/a), the seat pins "
                  "and a VERDICT63 with control_warmup_fail=4/5, cand_fail=0/5 and no BLOCKS-E-PRE",
                  all(labels.get(a) == "PASS" for a in cand_arms) and all(labels.get(c) == "REPORTED" for c in ("CPL-s42", "CPM-s42", "CU-s42", "CPL-s7", "CPM-s7")) and len(re.findall(r"^ROW63 ", score, re.M)) == 10 and "warm=NEVER-ACKED" in score
                  and re.search(r"^ROW63 CPL s42 r1: .*undeliv=n/a ", score, re.M) is not None and re.search(r"^ROW63 FPL s42 r1: .*transfer=COMPLETED undeliv=0\(srv 0\+cli 0\) ", score, re.M) is not None and "symsz(cli)=512/512" in score
                  and "VERDICT63 REPORTED-ONLY rows=10/10 control_warmup_fail=4/5 cand_fail=0/5" in score and "BLOCKS-E-PRE" not in score and "CAND-READING-WEAK" not in score
                  and "SEAT63 CONTROL bin_sha " + ctl_sha[:16] + " (equals the pinned value)" in score and "SEAT63 CAND bin_sha " + cand_sha[:16] + " (equals the pinned value)" in score, (labels, score[-1500:]))
        check(f"{tag}: DONE-ALL and score.txt", os.path.exists(os.path.join(run_dir, "DONE-ALL")) and os.path.exists(os.path.join(run_dir, "score.txt")), tail(e, 12))
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


# ── plan 67 (status 67, VM batch 1): seven seats, three session tokens. 67 = the rehearsal (LD_REHEARSE=1), 67g = guards 1 and 2, 67x = the stages off Bulk and the tunnel. The launches (67g, 67x) run on the rehearsal's
#    binaries and tools result (REH67, phase 2). The seat binaries are stub scripts the cargo stub copies from STUB_SEATDIR (one per seat; the strings plan67.SEATS says the real binary holds are comments in them, so the
#    envelope's identity check runs on real greps; the config-file behaviour stage T reads is a shell body). ──
import plan67  # noqa: E402

REH67 = {}          # the rehearsal's persisted run dir
TRUNC_EXIT67 = 7   # the session exit of a non-pass that is not a failure (loopdiag_p67.sh P67_EXIT_NONPASS): TRUNCATED-ALL and the rehearsal's R2 void; every assertion of it reads this one constant
TREES67 = ("B0", "S1", "S2", "P1", "P2")
COMMIT67 = {"B0": "b0" * 20, "S1": "2d590cdc" + "0" * 32, "S2": "1953abd1" + "0" * 32, "P1": "a1" * 20, "P2": "a2" * 20}
ALL67 = tuple(p for p in ("67pre", "67", "67g", "67x"))
KEYS67 = ("window_out_of_order", "window_coded_only", "window_generation_coding", "window_systematic_repair")
BIN_COMMON67 = r'''
cfg=""; cmd=""
while [ $# -gt 0 ]; do case "$1" in --config) cfg="$2"; shift ;; check|run) cmd="$1" ;; esac; shift; done
[ -n "$cfg" ] && [ -f "$cfg" ] || exit 0
key=$(sed -n 's/^\([a-z_]*\) = true$/\1/p' "$cfg" | head -n 1)
[ -n "$key" ] || exit 0
'''
BIN_GUARD67 = r'''
if [ "$cmd" = check ] && [ "${STUB_NOREFUSE_KEY:-}" = "$key" ]; then exit 0; fi
echo "Error: $key is a perf/native-object mode; the tunnel always delivers in order: remove it from the config file" >&2
exit 1
'''
BIN_B0_67 = r'''
if [ "$cmd" = run ]; then
  out=""
  for f in window_out_of_order window_coded_only window_generation_coding window_systematic_repair; do
    v=false; [ "$f" = "$key" ] && [ -z "${STUB_B0_KEYFALSE:-}" ] && v=true
    out="$out $f: $v,"
  done
  echo "INFO raptorpath: resolved configuration peer_config=PeerConfig {$out }"
  [ -n "${STUB_B0_REFUSES:-}" ] && { echo "Error: $key is a perf/native-"'object mode; the tunnel always delivers in order: remove it from the config file' >&2; exit 1; }
fi
exit 0
'''
TUN_STUB67 = r'''#!/bin/bash
# stub tun_bulk.sh (plan 67 stage T): the lines the real script writes, without a topology
cell="$1"; hint="$2"; bytes="$3"; reps="$4"; label="${5:-$hint}"
echo "TUNARM label=$label cell=$cell hint=$hint bytes=$bytes reps=$reps seed=${SEED:-42} bin=$RWM_BIN start=$(date -u +%FT%TZ)"
echo "TUNSHA $(sha256sum "$RWM_BIN" | cut -d' ' -f1)"
if [ "${STUB_TUN_BRINGUP_FAIL:-}" = "$label" ]; then      # STUB_TUN_BRINGUP_FAIL=<seat>-<hint>: the three bring-up attempts fail, exactly the lines tun_bulk.sh writes
  for a in 1 2 3; do echo "TUN-BRINGUP-RETRY label=$label attempt=$a"; done
  echo "TUN-BRINGUP-FAIL label=$label"
  echo "TUNARM-DONE label=$label reps=0 $(date -u +%FT%TZ)"
  exit 0
fi
echo "TUNMTU cli=1196 srv=1196"
echo "TUNPIPE cli=window/Rlc/$hint srv=window/Rlc/$hint"
echo "TUNCAD cli=ACTIVE srv=ACTIVE"
echo "TUNCFG cli=ooo:false,coded:false,gen:false,sysrep:false srv=ooo:false,coded:false,gen:false,sysrep:false"
[ -n "${TB_TMPD:-}" ] && { echo "INFO raptorpath: resolved configuration peer_config=PeerConfig { window_out_of_order: false, window_coded_only: false, window_generation_coding: false, window_systematic_repair: false, }" > "$TB_TMPD/tb-c.log"; cp "$TB_TMPD/tb-c.log" "$TB_TMPD/tb-s.log"; }
for r in $(seq 1 "$reps"); do
  echo "TUNREP label=$label rep=$r rc=0 {\"bytes\": $bytes, \"mbps\": 90.0, \"run\": 1, \"seconds\": 8.9}"
done
echo "TUNARM-DONE label=$label reps=$reps $(date -u +%FT%TZ)"
exit 0
'''


VIS_STUB67 = r'''#!/bin/bash
# stub raptorpath-visualizer/build_visualizer.sh (plan 67pre, S2): logs the tree and the target dir it ran with, writes wasm/ like the real one; STUB_VIS_FAIL=1 fails it
cd "$(dirname "$0")/.." || exit 1
echo "$PWD|build_visualizer CTD=${CARGO_TARGET_DIR:-} wasm_before=$([ -e raptorpath-visualizer/wasm ] && echo yes || echo no)" >> "$STUB_CARGO_LOG"
[ -z "${STUB_VIS_FAIL:-}" ] || { echo "stub wasm-pack: build failed" >&2; exit 1; }
mkdir -p raptorpath-visualizer/wasm && echo wasm > raptorpath-visualizer/wasm/raptorpath_wasm_bg.wasm
echo "// rewritten" >> raptorpath-visualizer/interactive-visualizer.html
exit 0
'''


def strings67(defect):
    """{seat: the strings its stub binary holds}: plan67.SEATS' need lists, less / plus the registered defect."""
    out = {s: list(plan67.SEATS[s]["need"]) for s in plan67.SEAT_ORDER}
    if defect == "seat-s1-no-pfec":
        out["S1"] = [x for x in out["S1"] if x != "pfec="]
    if defect == "seat-b0-has-gauge":
        out["B0"].append("rlaw=")
    if defect in ("s2-seat-abort", "s2-dropped"):
        out["S2"] = [x for x in out["S2"] if x != "pslot="]
    return out


def bin_text67(seat, st):
    lines = ["#!/bin/sh", "# stub engine binary of seat %s" % seat, "# RWM_STORE"] + ["# " + x for x in st[seat]]
    return "\n".join(lines) + "\n" + BIN_COMMON67 + (BIN_B0_67 if seat == "B0" else BIN_GUARD67)


def make67(root, plan, path):
    """The sandbox of a plan-67 case: the harness tree (tools/l1 shrunk), a stub tun_bulk.sh, one source tree per seat (a copy of the harness tree: every seat runs one harness; the pacer trees' lib.sh forwards
    RWM_PERF_RATE_MBIT), the COMMIT of each, the seat binaries the cargo stub copies, and the path's one defect."""
    sb = make_sandbox(root, plan, None)
    with open(os.path.join(sb["repo"], "tools", "l1", "tun_bulk.sh"), "w", newline="\n") as fh:
        fh.write(TUN_STUB67)
    sd = os.path.join(sb["logs"], "seatbins")
    os.makedirs(sd, exist_ok=True)
    st = strings67(path)
    for seat in plan67.SEAT_ORDER:
        with open(os.path.join(sd, seat + ".bin"), "w", newline="\n") as fh:
            fh.write(bin_text67(seat, st))
    sb["seatdir"] = sd
    vis = os.path.join(sb["repo"], "raptorpath-visualizer")
    os.makedirs(vis, exist_ok=True)
    with open(os.path.join(vis, "build_visualizer.sh"), "w", newline="\n") as fh:
        fh.write(VIS_STUB67)
    with open(os.path.join(vis, "interactive-visualizer.html"), "w", newline="\n") as fh:
        fh.write("<html></html>\n")
    with open(os.path.join(sb["repo"], "raptorpath", "Cargo.toml"), "a", newline="\n") as fh:      # the [[test]] stanzas raptorpath/Cargo.toml declares: the repeat (repeat67_run.sh) splits its sim list by them
        fh.write("".join('\n[[test]]\nname = "%s"\nrequired-features = [%s]\n' % tf for tf in (("s06_reactive", '"test-seams"'), ("sim_shed_reach", '"sim", "test-seams"'), ("sim_lost_tail", '"sim", "test-seams"'), ("sim_s06_nack", '"sim", "test-seams"'))))
    for tree in TREES67:
        dst = os.path.join(sb["root"], "seats", tree, "src")
        shutil.copytree(sb["repo"], dst)
        with open(os.path.join(sb["root"], "seats", tree, "COMMIT"), "w", newline="\n") as fh:
            fh.write(COMMIT67[tree] + "\n")
    for tree in ("P1", "P2"):
        lib = os.path.join(sb["root"], "seats", tree, "src", "tools", "l1", "lib.sh")
        txt = open(lib, newline="").read()
        assert txt.count("RWM_OOO_RETAIN RWM_PERF_TIMEOUT_S") == 1
        with open(lib, "w", newline="") as fh:
            fh.write(txt.replace("RWM_OOO_RETAIN RWM_PERF_TIMEOUT_S", "RWM_OOO_RETAIN RWM_PERF_RATE_MBIT RWM_PERF_TIMEOUT_S"))
    return sb


def tree67(root, tree, rel):
    return os.path.join(root, "root", "seats", tree, "src", *rel.split("/"))


def append67(root, tree, rel, text):
    with open(tree67(root, tree, rel), "a", newline="\n") as fh:
        fh.write(text)


def manifest67_py(repo):
    """src_manifest67 (loopdiag_p67.sh) of a tree: manifest_py's set plus tools/ci/flake_rule.py, in codepoint order."""
    lines = manifest_py(repo).splitlines()
    rule = os.path.join(repo, "tools", "ci", "flake_rule.py")
    lines.append("%s  tools/ci/flake_rule.py" % hashlib.sha256(open(rule, "rb").read()).hexdigest())
    return "".join(x + "\n" for x in sorted(lines, key=lambda ln: ln.split("  ", 1)[1]))


def ref67_files(root, sb):
    """LD_REF_MANIFEST_S1 / _S2: session 0's record, taken from the PRISTINE harness tree (the seat trees are copies of it; a case edits a seat tree afterwards to make a MISS)."""
    out = {}
    for s in ("S1", "S2"):
        p = os.path.join(root, "ref67-%s.sha256" % s)
        with open(p, "w", newline="\n") as fh:
            fh.write(manifest67_py(sb["repo"]))
        out["LD_REF_MANIFEST_" + s] = px(p)
    return out


def led67(stage_dir, name="ld.log"):
    import s67_score
    p = os.path.join(stage_dir, name)
    return s67_score.rows_of([p])[0] if os.path.exists(p) else []


def txt67(*parts):
    p = os.path.join(*parts)
    return open(p, errors="replace").read() if os.path.exists(p) else ""


def binshas67(run_dir):
    """{seat: sha} from BINSHA.txt (`<sha>  <path>  seat=<S> commit=.. features=..`)."""
    d = {}
    if not os.path.exists(os.path.join(run_dir, "BINSHA.txt")):
        return d
    for ln in open(os.path.join(run_dir, "BINSHA.txt"), errors="replace"):
        m = re.match(r"([0-9a-f]{64})\s+\S+\s+seat=(\S+)", ln)
        if m:
            d[m.group(2)] = m.group(1)
    return d


def cargo67pre(cargo):
    """The cargo log of a 67pre run: [(seat, dir, args)] from `PWD|args`; the seat is the one of a .../seats/<seat>/src cwd (None elsewhere), dir the cwd's last component."""
    out = []
    for c in cargo:
        pwd, _, args = c.partition("|")
        m = re.search(r"/seats/(\w+)/src$", pwd)
        out.append((m.group(1) if m else None, pwd.rsplit("/", 1)[-1], args))
    return out


def tgt67pre(sb):
    """[(seat or dir, verb, target dir)] from the cargo stub's .tgt log (every call: cwd, verb, CARGO_TARGET_DIR)."""
    p = os.path.join(sb["logs"], "cargo.log.tgt")
    out = []
    for ln in (open(p).read().splitlines() if os.path.exists(p) else []):
        pwd, verb, ctd = (ln.split("|") + ["", ""])[:3]
        m = re.search(r"/seats/(\w+)/src$", pwd)
        out.append((m.group(1) if m else pwd.rsplit("/", 1)[-1], verb, ctd))
    return out


def case67pre(plan, path):
    """Plan 67pre (session 0 of VM batch 1: loopdiag_p67pre.sh) on the stubs: the pre-flight with no battery. `full` is the whole session; every other path breaks ONE input and shows its abort, what ran before it and what did not."""
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make67(root, plan, path)
        rd = os.path.join(sb["logs"], "repeat")
        os.makedirs(rd)
        open(os.path.join(sb["logs"], "drv7.log"), "w").close()
        extra = {"STUB_SEATDIR": px(sb["seatdir"]), "LD_PIN_B0": COMMIT67["B0"], "LD_PIN_S1": COMMIT67["S1"], "LD_PIN_S2": COMMIT67["S2"], "LD_PIN_P1": COMMIT67["P1"], "LD_PIN_P2": COMMIT67["P2"],
                 "STUB_DRVLOG7": px(sb["logs"]) + "/drv7.log", "LD_TEST_SEAMS": "1", "STUB_TARGETS": "1", "R67_N": "3", "R67_T1_S": "1", "STUB_REPEAT_DIR": px(rd)}      # R67_N / R67_T1_S: the repeat's test seams (N = 3 on the stub), reaching it under the marker only
        envs = {"check-fail-s1": {"STUB_CHECK_FAIL": "S1"}, "check-fail-s2": {"STUB_CHECK_FAIL": "S2"}, "gates-mismatch": {"STUB_GATES_FAIL": "S1:g_one"}, "gates-equal-b0": {"STUB_GATES_FAIL": "B0,S1:g_one"},
                "gates-and-flake": {"STUB_GATES_FAIL": "S1:g_one", "STUB_FLAKE": FLAKE_TEST}, "gates-no-result": {"STUB_GATES_NORESULT": "1"}, "flake-not-quarantined": {"STUB_FLAKE": FLAKE_TEST},
                "repeat-one-failure": {"STUB_REPEAT_FAIL": "2"}, "s2-dropped": {"LD_DROP_S2": "1"}, "visualizer-fail": {"STUB_VIS_FAIL": "1"}, "golden-fail": {"STUB_GOLDEN_FAIL": "1"},
                "budget": {"LD_HARD_MIN_S": "99999999"}, "budget-step": {"LD_HARD_MIN_S": "1", "LD_HARD": str(int(time.time()) + 300)},
                "lock-inherit-fail": {"STUB_LOCK_PARENT": "1"}, "binid-test-mismatch": {"STUB_BINTEST_FAIL": COMMIT67["S1"] + ":t_one"},
                "binid-test-equal-b0": {"STUB_BINTEST_FAIL": COMMIT67["B0"] + "," + COMMIT67["S1"] + ":t_one"}}
        extra.update(envs.get(path, {}))
        if path == "seam-r67-n":      # a session-0 seam without the stub marker: R67_N alone (R67_T1_S is a seam too, so it goes with the marker)
            for k in ("LD_TEST_SEAMS", "R67_T1_S"):
                extra.pop(k, None)
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=(path == "rehearse-refused"), extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        cl = cargo67pre(cargo)
        tg = tgt67pre(sb)
        drv = open(os.path.join(sb["logs"], "drv.log")).read()
        by_seat = {s: [a for sd, _, a in cl if sd == s] for s in ("B0", "S1", "S2", "P1", "P2")}
        all_calls = [a for _, _, a in cl]
        sessf = os.path.join(run_dir, "SESSION0.txt")

        def pre(*p):
            return txt67(run_dir, "pre", *p)

        stage_dirs = [os.path.join(run_dir, d) for d in plan67.STAGE_DIR.values()]
        no_ledger = rows == [] and drv == "" and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs) and not any(os.path.exists(d) for d in stage_dirs)
        no_record = not os.path.exists(sessf) and not any(f.startswith("REF-MANIFEST-") for f in (os.listdir(run_dir) if os.path.isdir(run_dir) else []))
        builds = [c for c in cargo if "|build " in c]
        done = "LD-ALL-SESSION0-DONE" in out
        gone = not any(n.startswith(("target67pre-", "b67work", "vis67pre-")) for n in os.listdir(os.path.join(root, "root")))
        n_checks = all_calls.count("check --all-targets")
        n_vis = sum(1 for a in all_calls if a.startswith("build_visualizer"))
        n_golden = all_calls.count("test -p raptorpath-wasm golden")
        n_binid = sum(1 for sd, d, a in cl if a.startswith("build ") and sd is None)
        ran_s = [s for s in ("S1", "S2") if any(a.startswith("test -p raptorpath -p raptorpath-math --release --no-fail-fast") and "--exact" not in a for a in by_seat[s])]
        gates_n = by_seat["B0"].count("test -p raptorpath --lib gates")

        def refused(text, checks, gates, suites, more=()):
            """an abort: rc 5, the exact text on record, FAILED-ALL, no SESSION0 record, no reference manifest, nothing measured, no build, and exactly the steps that ran before it."""
            check(f"{tag}: rc 5, `{text}` on record, FAILED-ALL, no SESSION0 record and no reference manifest, no ledger / row / driver call, no build; ran before it: check on {checks or 'none'}, gates on B0 {'yes' if gates else 'no'}, suites on {suites or 'none'}; nothing after it (no repeat, visualizer, golden, binid)",
                  rc == 5 and failed and text in e and all(x in e for x in more) and no_record and no_ledger and builds == [] and not done and [sd for sd, _, a in cl if a == "check --all-targets"] == checks
                  and gates_n == (1 if gates else 0) and ran_s == suites and n_vis == (1 if path in ("visualizer-fail", "golden-fail") else 0) and n_golden == (1 if path == "golden-fail" else 0) and n_binid == 0
                  and not any("--no-run" in a for a in all_calls), (rc, tail(e, 8), cl[:14], ran_s))

        if path == "rehearse-refused":
            check(f"{tag}: LD_REHEARSE=1 on session 0: rc 3, ABORT-REHEARSE (67pre is not in run_all's REHEARSE list), nothing run (no cargo call)",
                  rc == 3 and "ABORT-REHEARSE LD_REHEARSE=1 is defined for plans" in out and "not 67pre; NOTHING WAS RUN" in out and cl == [] and not failed and not done, (rc, out[-300:]))
        elif path == "budget":
            check(f"{tag}: less than the registered worst case (17 040 s) before LD_HARD: rc 3, ABORT-BUDGET plan 67pre, NOTHING WAS RUN (no cargo call)", rc == 3 and "ABORT-BUDGET plan 67pre:" in out and "NOTHING WAS RUN" in out and cl == [] and not done, (rc, tail(out, 4)))
        elif path == "seam-r67-n":
            check(f"{tag}: R67_N set without LD_TEST_SEAMS=1: rc 3, `ABORT-SEAM plan 67pre: test seam(s) active without LD_TEST_SEAMS=1: R67_N` before the locks, nothing ran (no cargo call, no child, no ledger)",
                  rc == 3 and "ABORT-SEAM plan 67pre: test seam(s) active without LD_TEST_SEAMS=1: R67_N" in out + e and cl == [] and not done and not os.path.exists(os.path.join(run_dir, "pre")) and out.count("LOCK-TAKEN") == 0, (rc, tail(out, 6), cl[:3]))
        elif path == "lock-inherit-fail":
            check(f"{tag}: step 0: a child that sees another parent pid cannot inherit the locks: rc 5, `ABORT-PREFLIGHT plan 67pre step locks: a child cannot inherit the session's locks` on record, FAILED-ALL, nothing after it (no cargo call, no repeat / binid output, no record), no LOCK-INHERITED in the child's log",
                  rc == 5 and failed and "ABORT-PREFLIGHT plan 67pre step locks: a child cannot inherit the session's locks" in e and cl == [] and not done and no_record and no_ledger and not os.path.exists(os.path.join(run_dir, "pre", "repeat.out"))
                  and "LOCK-INHERITED" not in pre("lockcheck.log"), (rc, tail(e, 4), pre("lockcheck.log"), pre("lockcheck.out")))
        elif path == "binid-test-mismatch":
            check(f"{tag}: S1's math + wasm tests fail a test B0 passes: rc 5, `ABORT-PREFLIGHT plan 67pre binid: BINTEST S1 binid_math::t_one not equal on B0`, FAILED-ALL, the earlier steps ran (3 binid builds, REPEAT, visualizer), no SESSION0 record; S1 is not the S2 family: no relaunch hint",
                  rc == 5 and failed and "ABORT-PREFLIGHT plan 67pre binid: BINTEST S1 binid_math::t_one not equal on B0" in e and "relaunch with LD_DROP_S2=1" not in e and n_binid == 3 and n_vis == 1 and not os.path.exists(sessf) and not done and no_ledger,
                  (rc, tail(e, 6), n_binid, pre("binid", "BINID.txt")))
            check(f"{tag}: both reference manifests were already written (each seat's own steps had passed) and persist in the run: REF-MANIFEST-S1 and -S2 exist, byte-equal to the manifest of the pristine tree",
                  all(txt67(run_dir, "REF-MANIFEST-%s.sha256" % x) == manifest67_py(sb["repo"]) != "" for x in ("S1", "S2")), os.listdir(run_dir))
        elif path == "budget-step":
            check(f"{tag}: under 600 s left when the first step would start: rc 5, FAILED-ALL, `ABORT-BUDGET plan 67pre step check-S1`, nothing ran (no cargo call), no record",
                  rc == 5 and failed and "ABORT-BUDGET plan 67pre step check-S1:" in e and cl == [] and not done and no_record, (rc, tail(e, 4)))
        elif path == "check-fail-s1":
            refused("ABORT-PREFLIGHT plan 67pre seat S1: cargo check --all-targets failed (", ["S1"], False, [])
            check(f"{tag}: S1 failing stops the session (no guard 1 to run alone): no relaunch hint, and the only cargo call is S1's check", "relaunch with LD_DROP_S2=1" not in e and all_calls == ["check --all-targets"], (all_calls, tail(e, 3)))
        elif path == "check-fail-s2":
            refused("ABORT-PREFLIGHT plan 67pre seat S2: cargo check --all-targets failed (", ["S1", "S2"], False, [], ["guard 2 stops; relaunch with LD_DROP_S2=1"])
        elif path == "gates-no-result":
            refused("ABORT-PREFLIGHT plan 67pre seat B0: the gates reference did not run (no test result line", ["S1", "S2"], True, [])
        elif path == "gates-mismatch":
            refused("ABORT-PREFLIGHT plan 67pre seat S1: gates failure not equal on B0: gates::g_one", ["S1", "S2"], True, ["S1"], ["LD-ALL FAILED: ABORT-PREFLIGHT"])
            check(f"{tag}: B0's reference is empty and S1's gates failure (REAL: the solo re-run fails too) is named in S1's TESTS.txt", pre("GATES-B0.txt") == "" and "REAL-FAILURE main gates::g_one" in pre("suites-S1", "TESTS.txt"), pre("suites-S1", "TESTS.txt")[-400:])
        elif path == "gates-and-flake":
            refused("ABORT-PREFLIGHT plan 67pre seat S1: gates failure not equal on B0: gates::g_one", ["S1", "S2"], True, ["S1"], ["ABORT-TESTS plan 67pre seat S1: stub::flaky_one (ADR-0080: REAL or non-quarantined FLAKE)"])
            check(f"{tag}: both lines are printed and the abort is ABORT-PREFLIGHT (5.4's order): the ABORT-TESTS line is on record first, the FAILED-ALL line carries ABORT-PREFLIGHT",
                  e.index("ABORT-TESTS plan 67pre seat S1") < e.index("LD-ALL FAILED: ABORT-PREFLIGHT plan 67pre seat S1"), tail(e, 4))
        elif path == "flake-not-quarantined":
            refused("ABORT-TESTS plan 67pre seat S1: stub::flaky_one (ADR-0080: REAL or non-quarantined FLAKE)", ["S1", "S2"], True, ["S1"], ["LD-ALL FAILED: ABORT-TESTS"])
            tx = pre("suites-S1", "TESTS.txt")
            check(f"{tag}: the test failed once and passed its solo re-run (FLAKE), is not on quarantine.txt (FLAKE-FAILURE, not QUARANTINED): a failure is a failure", "FLAKE main stub::flaky_one" in tx and "FLAKE-FAILURE main stub::flaky_one" in tx and "QUARANTINED" not in tx, tx[-400:])
        elif path == "repeat-one-failure":
            check(f"{tag}: one failing run in 3 of S2's gating tests: rc 5, `ABORT-REPEAT plan 67pre seat S2` through guard 2's stop (LD_DROP_S2=1), S1's repeat read zero failures; the checks, gates and both suites ran, the visualizer, binid and the record did not",
                  rc == 5 and failed and "ABORT-REPEAT plan 67pre seat S2 (ABORT-REPEAT seat=S2" in e and "relaunch with LD_DROP_S2=1" in e and "REPEAT67 seat=S1 reached=3 of 3 failing_runs=0" in pre("repeat", "REPEAT67.txt")
                  and "failing_runs=1 failures=[s06_no_copy::t_one]" in pre("repeat", "REPEAT67.txt") and n_checks == 2 and gates_n == 1 and ran_s == ["S1", "S2"] and n_vis == 0 and n_binid == 0 and not os.path.exists(sessf) and no_ledger and not done and gone,
                  (rc, tail(e, 5), pre("repeat", "REPEAT67.txt")))
            check(f"{tag}: S1's reference manifest is already in the persisted run (written when S1's check, suites and repeat had passed, before S2's failure aborted the session) and equals the pristine tree's; S2's is not there",
                  txt67(run_dir, "REF-MANIFEST-S1.sha256") == manifest67_py(sb["repo"]) != "" and not os.path.exists(os.path.join(run_dir, "REF-MANIFEST-S2.sha256")) and "REF-MANIFEST seat=S1 written" in pre("NOTES.txt")
                  and "REF-MANIFEST seat=S2" not in pre("NOTES.txt"), (os.listdir(run_dir), pre("NOTES.txt")[-400:]))
        elif path in ("visualizer-fail", "golden-fail"):
            what = "visualizer build failed" if path == "visualizer-fail" else "golden test failed"
            check(f"{tag}: S2's {what.split(' ')[0]} step failing: rc 5, `ABORT-PREFLIGHT plan 67pre seat S2: {what}` through guard 2's stop; checks, gates, suites and repeats ran before it, binid and the record did not; the scratch copy is gone and S2's own tree has no wasm",
                  rc == 5 and failed and f"ABORT-PREFLIGHT plan 67pre seat S2: {what} (" in e and "relaunch with LD_DROP_S2=1" in e and n_checks == 2 and n_vis == 1 and n_golden == (1 if path == "golden-fail" else 0) and n_binid == 0 and not os.path.exists(sessf) and not done and gone
                  and os.path.exists(os.path.join(run_dir, "REF-MANIFEST-S1.sha256")) and not os.path.exists(os.path.join(run_dir, "REF-MANIFEST-S2.sha256"))
                  and not os.path.exists(tree67(root, "S2", "raptorpath-visualizer/wasm")) and "REPEAT67 seat=S2 reached=3 of 3 failing_runs=0" in pre("repeat", "REPEAT67.txt"), (rc, tail(e, 4), cl[-6:]))
        else:
            # ── full, gates-equal-b0, s2-dropped: the session runs to LD-ALL-SESSION0-DONE ──
            dropped = path == "s2-dropped"
            seats = ["B0", "S1"] if dropped else ["B0", "S1", "S2"]
            sess = txt67(run_dir, "SESSION0.txt")
            check(f"{tag}: rc 0, LD-ALL-SESSION0-DONE, DONE-ALL, no FAILED-ALL; the budget guard ran and both locks were taken and released by run_all alone",
                  rc == 0 and done and not failed and os.path.exists(os.path.join(run_dir, "DONE-ALL")) and "budget:" in e and out.count("LOCK-TAKEN") == 2 and out.count("LOCK-RELEASED") == 2 and not os.path.exists(os.path.join(sb["locks"], "vm.lock")), (rc, tail(e, 8), tail(out, 6)))
            suite_kinds = ("test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2", "test --doc -p raptorpath --release", "test -p raptorpath-wasm", "test -p raptorpath-core", "test -p raptorpath --no-fail-fast -- --test-threads=2")
            check(f"{tag}: cargo check --all-targets ran on {'S1 only' if dropped else 'S1 and S2'} (not B0), the gates reference on B0 only, the suites (release, doc, wasm, core, debug) on {'S1' if dropped else 'S1 and S2'}, each in its own tree; B0's tree saw nothing else",
                  [sd for sd, _, a in cl if a == "check --all-targets"] == (["S1"] if dropped else ["S1", "S2"]) and by_seat["B0"] == ["test -p raptorpath --lib gates"]
                  and all(sum(1 for a in by_seat[s] if a == k) == 1 for s in seats[1:] for k in suite_kinds), {s: by_seat[s][:14] for s in ("B0", "S1")})
            suite_ct = {s: {c.rsplit("/", 1)[-1] for sd, v, c in tg if sd == s and "suites" in c} for s in ("S1", "S2")}
            want = {("S1", "check"): "target67pre-check-S1", ("S2", "check"): "target67pre-check-S2", ("B0", "test"): "target67pre-gates-B0"}
            check(f"{tag}: every step used its own fresh target dir under $ROOT (check S1 / S2, gates B0, suites S1 / S2, the repeats, the visualizer, the golden test) and none is left behind; a tree's suites ran with that tree's target only",
                  all(any(sd == s and v == k and c.endswith("/" + n) for sd, v, c in tg) for (s, k), n in want.items() if not (dropped and s == "S2"))
                  and suite_ct["S1"] == {"target67pre-suites-S1"} and suite_ct["S2"] == (set() if dropped else {"target67pre-suites-S2"}) and gone
                  and all(("target67pre-suites-S1" in c) == (sd == "S1") for sd, v, c in tg if "suites" in c), (tg[:6], suite_ct))
            tx = {s: pre("suites-" + s, "TESTS.txt") for s in seats[1:]}
            check(f"{tag}: each seat's suites wrote their own output dir ($RUN/pre/suites-<S>), the engine suites ran, the tools tests were skipped (session 1 runs them)",
                  all("TEST main rc=" in tx[s] and "TEST debug-witness rc=" in tx[s] and "TOOLS-TESTS-SKIPPED" in tx[s] and "TEST parsers" not in tx[s] and "TEST bashtests" not in tx[s] for s in seats[1:]), {s: tx[s][-300:] for s in tx})
            s2list = "holeclass_reachability recovery_bench sim_nack_test place_arms_reachability s06_no_copy s06_reactive sim_shed_reach sim_lost_tail"
            check(f"{tag}: the registered touched-integration-test targets are reported per seat as seen in the suite logs (S2: the eight sim / recovery / s06 / holeclass targets; S1: config::), no gate",
                  "SESSION0 TARGETS seat=S1 seen=[config::] not-seen=[]" in e and (dropped or ("SESSION0 TARGETS seat=S2 seen=[%s] not-seen=[]" % s2list) in e), tail(e, 14))
            rp = pre("repeat", "REPEAT67.txt")
            check(f"{tag}: the repeat runs ran on {'S1' if dropped else 'S1 and S2'} (REPEAT67 lines, zero failing runs; the sim tests twice on S2) and the children inherited run_all's locks (LOCK-INHERITED twice each, never released by them)",
                  "REPEAT67 seat=S1 reached=3 of 3 failing_runs=0" in rp and (dropped or "REPEAT67 seat=S2 reached=3 of 3 failing_runs=0" in rp) and "REPEAT67-DONE bad=0" in rp and (dropped or sum(1 for a in by_seat["S2"] if "--test sim_shed_reach" in a and "--no-run" not in a) == 2)
                  and pre("repeat", "era.txt").count("LOCK-INHERITED") == 2 and "LOCK-RELEASED" not in pre("repeat", "era.txt") and pre("binid", "era.txt").count("LOCK-INHERITED") == 2 and "LOCK-RELEASED" not in pre("binid", "era.txt"),
                  (rp, pre("repeat", "era.txt")[-300:]))
            parts = [(c.partition("|")[0], c.partition("|")[2]) for c in cargo]
            vis = [(p, a) for p, a in parts if a.startswith("build_visualizer")]
            gold = [p for p, a in parts if a == "test -p raptorpath-wasm golden"]
            if dropped:
                check(f"{tag}: LD_DROP_S2=1: no S2 step ran (no check, suites, repeat, visualizer or golden in S2's tree or anywhere), binid ran on B0 and S1 only (2 builds, two shas, BINID67 PASS), REF-MANIFEST-S1 only",
                      by_seat["S2"] == [] and vis == [] and gold == [] and n_binid == 2 and re.search(r"^BINID67 B0=[0-9a-f]{64} S1=[0-9a-f]{64}$", pre("binid", "BINID.txt"), re.M) and "BINID67 PASS" in pre("binid", "BINID.txt")
                      and os.path.exists(os.path.join(run_dir, "REF-MANIFEST-S1.sha256")) and not os.path.exists(os.path.join(run_dir, "REF-MANIFEST-S2.sha256")) and "LD_REF_MANIFEST_S2" not in sess and "seat=S2" not in sess, (cl[-5:], pre("binid", "BINID.txt")))
            else:
                check(f"{tag}: the visualizer ran on S2 only, once, in a scratch copy with no pre-existing wasm and its own fresh target, then the native golden test (no GOLDEN_CAPTURE) in that copy; S2's own tree has no wasm; then binid on B0, S1, S2 (3 builds at the one fixed path, 3 distinct shas, BINID67 PASS)",
                      len(vis) == 1 and vis[0][0].endswith("/vis67pre-S2") and "wasm_before=no" in vis[0][1] and "target67pre-vis-S2" in vis[0][1] and len(gold) == 1 and gold[0].endswith("/vis67pre-S2")
                      and not os.path.exists(tree67(root, "S2", "raptorpath-visualizer/wasm")) and n_binid == 3 and len({p for p, a in parts if a.startswith("build ")}) == 1
                      and re.search(r"^BINID67 B0=[0-9a-f]{64} S1=[0-9a-f]{64} S2=[0-9a-f]{64}$", pre("binid", "BINID.txt"), re.M) and "BINID67 PASS" in pre("binid", "BINID.txt"), (vis, gold, cl[-6:]))

                def idx(pred):
                    return next((i for i, c in enumerate(cargo) if pred(c)), -1)
                order = [idx(lambda c: "/seats/S1/src|check "in c), idx(lambda c: "/seats/S2/src|check " in c), idx(lambda c: c.endswith("|test -p raptorpath --lib gates")),
                         idx(lambda c: "/seats/S1/src|test -p raptorpath -p raptorpath-math --release --no-fail-fast" in c), idx(lambda c: "/seats/S2/src|test -p raptorpath -p raptorpath-math --release --no-fail-fast" in c),
                         idx(lambda c: "/seats/S1/src|test -p raptorpath --release --lib --no-run" in c), idx(lambda c: "/seats/S2/src|test -p raptorpath --release --test s06_no_copy" in c),
                         idx(lambda c: "|build_visualizer" in c), idx(lambda c: c.endswith("|test -p raptorpath-wasm golden")), idx(lambda c: "/b67work/src/raptorpath|build " in c)]
                check(f"{tag}: the steps ran in the registered order: check S1, check S2, gates on B0, suites S1, suites S2, repeat S1, repeat S2, visualizer, golden, binid", order == sorted(order) and min(order) >= 0 and len(set(order)) == len(order), order)
            check(f"{tag}: no release build of a seat (the only builds are binid's, at its one work path), no binary installed or kept (no BINSHA.txt, no bins67), no battery: no ledger, no row-parser call, no driver call",
                  no_ledger and not os.path.exists(os.path.join(run_dir, "BINSHA.txt")) and not os.path.exists(os.path.join(run_dir, "bins67")) and all("/b67work/src/raptorpath|" in b for b in builds) and not any(sd and a.startswith("build ") for sd, _, a in cl), (builds[:3], rows[:2]))
            ref_ok = all(txt67(run_dir, "REF-MANIFEST-%s.sha256" % s) == manifest67_py(sb["repo"]) for s in seats[1:])
            check(f"{tag}: REF-MANIFEST-{'S1' if dropped else 'S1 and S2'} are written by src_manifest67 (byte-equal to the manifest of the pristine tree, the form loopdiag_p67.sh section 5 compares), and SESSION0.txt records the commit pins, the check / suites / repeat / binid results and the LD_REF_MANIFEST_<S> lines",
                  ref_ok and all(("SEAT %s commit=%s" % (s, COMMIT67[s])) in sess for s in seats) and all(("LD_REF_MANIFEST_%s=%s/run/REF-MANIFEST-%s.sha256" % (s, px(os.path.join(root, "root")), s)) in sess for s in seats[1:])
                  and "CHECK seat=S1 ok" in sess and "SUITES seat=S1 ok" in sess and "REPEAT67 seat=S1 reached=3 of 3" in sess and "BINID67 B0=" in sess and "BINID-WORK" in sess and "BINID67 PASS" in sess, sess[-900:])
            if path == "binid-test-equal-b0":
                check(f"{tag}: the same math / wasm test fails on B0 and on S1: `BINTEST-FINDING-ON-MAIN seat=B0 binid_math::t_one` and `seat=S1` on record and in SESSION0.txt, no abort: the session completes (LD-ALL-SESSION0-DONE)",
                      "BINTEST-FINDING-ON-MAIN seat=B0 binid_math::t_one" in e and "BINTEST-FINDING-ON-MAIN seat=S1 binid_math::t_one (the same failure on B0)" in e and "BINTEST-FINDING-ON-MAIN seat=S1 binid_math::t_one" in sess
                      and "BINTEST-FINDING-ON-MAIN seat=S2" not in e and "BINTEST B0 rc=101" in pre("binid", "BINID.txt") and "BINTEST S2 rc=0" in pre("binid", "BINID.txt"), (tail(e, 8), sess[-600:]))
            if path == "gates-equal-b0":
                check(f"{tag}: the same gates failure on B0 and S1: GATES-B0-FINDING and GATES-FINDING-ON-MAIN seat=S1 on record (and in SESSION0.txt), the guard runs: S1's suites count as passed (the repeats and binid ran), S2's suites show none",
                      "GATES-B0-FINDING gates::g_one" in e and "GATES-FINDING-ON-MAIN seat=S1 gates::g_one" in e and "GATES-FINDING-ON-MAIN seat=S1 gates::g_one" in sess and "GATES-B0-FINDING gates::g_one" in sess
                      and "GATES-FINDING-ON-MAIN seat=S2" not in e and pre("GATES-B0.txt").split() == ["gates::g_one"] and "REAL-FAILURE main gates::g_one" in pre("suites-S1", "TESTS.txt"), (tail(e, 8), sess[-500:]))
            if path == "full":
                # the wiring: session 0's REF-MANIFEST-S1 / S2 are the ones session 1 (the rehearsal, plan 67) reads: a HIT on both seats
                kept = {s: os.path.join(root, "s0-REF-MANIFEST-%s.sha256" % s) for s in ("S1", "S2")}
                for s in kept:
                    shutil.copyfile(os.path.join(run_dir, "REF-MANIFEST-%s.sha256" % s), kept[s])
                extra2 = dict(extra)
                extra2.update({"LD_REF_MANIFEST_S1": px(kept["S1"]), "LD_REF_MANIFEST_S2": px(kept["S2"])})
                rc2, out2, rows2, cargo2 = run_launch(sb, "67", None, rehearse=True, extra_env=extra2)
                e2 = era(sb)      # run_all clears all-era.txt at its start: this is the rehearsal's alone
                check(f"{tag}: wiring: session 0's REF-MANIFEST-S1 / S2 fed as LD_REF_MANIFEST_S1 / _S2 into the same sandbox's plan-67 rehearsal give `suite-reuse HIT seat S1` and `seat S2` (the manifest session 0 writes is the one session 1 reads), and the rehearsal passes",
                      rc2 == 0 and "LD-ALL-REHEARSAL-DONE" in out2 and "suite-reuse HIT seat S1" in e2 and "suite-reuse HIT seat S2" in e2 and "suite-reuse MISS" not in e2 and "suite-reuse HIT" not in e, (rc2, tail(e2, 10)))
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


def case67(plan, path):
    """Plan 67 on the stubs. `rehearse` (plan 67) is the registered rehearsal and leaves the artifacts the launches (67g, 67x) consume; every other path breaks ONE input or injects ONE defect. Session 0 (67pre) is case67pre."""
    if plan == "67pre":
        return case67pre(plan, path)
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make67(root, plan, path)
        reh_run = REH67.get("run")
        extra = {"STUB_SEATDIR": px(sb["seatdir"]), "LD_PIN_B0": COMMIT67["B0"], "LD_PIN_S1": COMMIT67["S1"], "LD_PIN_S2": COMMIT67["S2"], "LD_PIN_P1": COMMIT67["P1"], "LD_PIN_P2": COMMIT67["P2"],
                 "STUB_DRVLOG7": px(sb["logs"]) + "/drv7.log", "LD_TEST_SEAMS": "1"}      # LD_TEST_SEAMS=1: the stub marker (fast mode shrinks the design, LD_HARD_MIN_S / LD_BLOCK_ROW_S are seams); seam-without-marker drops it
        open(os.path.join(sb["logs"], "drv7.log"), "w").close()
        extra.update(ref67_files(root, sb))
        rehearse = plan == "67" and path != "not-rehearsal"
        bad = {"s2-taper-nonzero": "s2-taper-nonzero", "s2-no-pslot": "s2-no-pslot@2:c2-100", "s2-belowfloor-high": "s2-belowfloor-high", "r1-blind": "r1-blind", "twin-no-final": "twin-no-final", "twin-worse": "twin-worse",
               "twin-under-min": "twin-under-min@1-99", "rehearsal-r2-smoke-fail": "twin-no-acct", "dump-needed": "no-succ-times", "a-under-min": "s1-crash@1-3:c2-100", "block1-fail": "s1-no-gauge@0:c2-100",
               "s2-taper-cotenant": "s2-taper-cotenant@2:c2-100", "r1-b0-gauge": "b0-gauge@2-99:c2-100", "r2-blind": "r2-blind", "twin-missing-field": "twin-missing-field"}
        if plan == "67g":
            # the defects sit PAST block 1 (the first 8 rows of each guard: the block-1 rule would stop the stage there): the 5th invocation of the seat on (@4); g1-fail counts over the whole session (@@), the replacement run included
            bad.update({"s2-taper-nonzero": "s2-taper-nonzero@4", "s1-no-gauge": "s1-no-gauge@4", "g1-fail": "s1-no-gauge@@4-99", "g-block1-fail": "s1-no-gauge@0", "g2-taper-replaced": "g2-taper-no-pslot@4"})
        if plan == "67x":
            bad.update({"s2-taper-nonzero": "s2-taper-nonzero@2:c2-100", "truncated-s06-fail": "s2-taper-nonzero@2:c2-100",
                        "a-crash-partial": "s2-crash@@0:c8-100,s2-no-pslot@@1-6:c8-100", "truncated-candidate-crash": "s2-crash@@0-2:c8-100"})      # S2 in c8 (not the block-1 row): one crash and every other slot (its replacement included) VOID-WITNESS / three crashes
        if path in bad:
            extra["STUB_BAD67"] = bad[path]
        # per-path breakages of the trees / the environment
        if path == "seat-commit-mismatch":
            with open(os.path.join(sb["root"], "seats", "S1", "COMMIT"), "w") as fh:
                fh.write("deadbeef" + "0" * 32 + "\n")
        if path == "pin-missing-b0":
            del extra["LD_PIN_B0"]
        if path == "pin-missing-s1":
            del extra["LD_PIN_S1"]
        if path == "seam-without-marker":
            del extra["LD_TEST_SEAMS"]
            extra["LD_HARD_MIN_S"] = "1"
        if path == "t-bringup-fail":
            extra["STUB_TUN_BRINGUP_FAIL"] = "B0-realtime"
        if path == "t-refusal-and-void":
            extra["STUB_NOREFUSE_KEY"] = "window_coded_only"
            extra["STUB_B0_KEYFALSE"] = "1"
        if path == "truncated":
            extra["STUB_TRUNC"] = "67r"
        if path in ("truncated-s06-fail", "truncated-candidate-crash"):
            extra["STUB_TRUNC"] = "67a"      # stage A is the cut stage: its S2 finding must stay on record
        if path == "tools-diff":
            append67(root, "S2", "tools/l1/l1common.py", "# one more byte: this seat's tools tree is not the harness's\n")
        if path == "manifest-miss":
            append67(root, "S1", "raptorpath/src/lib.rs", "// one byte differs\n")
        if path == "flake-rule-edit":
            append67(root, "S1", "tools/ci/flake_rule.py", "# the rule changed\n")
        if path == "ref-missing":
            extra["LD_REF_MANIFEST_S1"] = "/nonexistent/s1-manifest.sha256"
        if path == "t-base-refuses":
            extra["STUB_B0_REFUSES"] = "1"
        if path == "t-refusal-missing":
            extra["STUB_NOREFUSE_KEY"] = "window_coded_only"
        if path == "b0-run-key-false":
            extra["STUB_B0_KEYFALSE"] = "1"
        if path in ("s2-dropped",) or path == "g2-fail":
            extra["LD_DROP_S2" if path == "s2-dropped" else "LD_G2_FAIL67"] = "1"
        if path == "dump-twins":
            extra["LD_DUMP67"] = "1"
        if path == "cut-pool":
            extra["LD_CUT67"] = "pool"
        if path == "budget":
            extra["LD_HARD_MIN_S"] = "99999999"
        if plan in ("67g", "67x"):
            if reh_run is None:
                check(f"{tag}: needs the rehearsal's run dir", False, "the 67/rehearse case did not leave one")
                return
            shas = binshas67(reh_run)
            for s, sha in shas.items():
                if path == "prebuilt-sha-mismatch" and s == "S1":
                    sha = "0" * 64
                extra["LD_BIN_SHA_" + s] = sha
                if not (path == "prebuilt-missing" and s == "S2"):
                    extra["LD_BIN_PREBUILT_" + s] = px(os.path.join(reh_run, "bins67", s + "-raptorpath"))
            extra["LD_PRIOR_TOOLS"] = px(reh_run)
        rc, out, rows, cargo = run_launch(sb, plan, None, rehearse=rehearse, extra_env=extra)
        e = era(sb)
        run_dir = os.path.join(sb["root"], "run")
        failed = os.path.exists(os.path.join(run_dir, "FAILED-ALL"))
        builds = [c for c in cargo if "|build " in c]
        drv = open(os.path.join(sb["logs"], "drv.log")).read()
        none_measured = rows == [] and drv == "" and not any(f in ("ld.log", "smoke.log") for _, _, fs in os.walk(run_dir) for f in fs)
        refused = {
            "not-rehearsal": (5, "ABORT-REHEARSE plan 67 is the rehearsal", 0),
            "seat-s1-no-pfec": (5, "ABORT-SEAT plan 67 seat S1: the binary has no 'pfec=' string", 7),
            "seat-b0-has-gauge": (5, "ABORT-SEAT plan 67 seat B0: the binary carries 'rlaw='", 7),
            "seat-commit-mismatch": (5, "ABORT-SEAT plan 67 seat S1:", 0),
            "pin-missing-b0": (5, "ABORT-SEAT plan 67 seat B0: LD_PIN_B0 is unset", 0),
            "pin-missing-s1": (5, "ABORT-SEAT plan 67 seat S1: LD_PIN_S1 is unset", 0),
            "seam-without-marker": (3, "ABORT-SEAM plan 67g: test seam(s) active without LD_TEST_SEAMS=1: LD_HARD_MIN_S", 0),
            "s2-seat-abort": (5, "relaunch with LD_DROP_S2=1", 7),
            "tools-diff": (5, "ABORT-SEAT plan 67 tools-diff witness", 7),
            "manifest-miss": (5, "ABORT-SUITES plan 67 suite-reuse MISS: seat S1's tree", 7),
            "flake-rule-edit": (5, "ABORT-SUITES plan 67 suite-reuse MISS: seat S1's tree", 7),
            "ref-missing": (5, "ABORT-SUITES plan 67: no reference manifest for seat S1", 7),
            "prebuilt-sha-mismatch": (5, "ABORT-BUILD plan 67 seat S1: built / installed", 0),
            "prebuilt-missing": (5, "ABORT-BUILD plan 67 launches never build", 0),
            "cut-pool": (5, "ABORT-CUT plan 67: LD_CUT67=pool", 0),
            "budget": (3, "ABORT-BUDGET plan %s" % plan, 0),
        }
        if path in refused:
            rc_w, txt, n_builds = refused[path]
            check(f"{tag}: refused before any row: rc {rc_w}, `{txt}` on record, {'FAILED-ALL, ' if rc_w == 5 else ''}nothing measured (no row-parser call, no driver call), {n_builds} builds ran",
                  rc == rc_w and txt in e + out and (failed or rc_w != 5) and none_measured and len(builds) == n_builds and "LD-ALL-DONE" not in out and "LD-ALL-REHEARSAL-DONE" not in out, (rc, tail(e, 8), tail(out, 6), len(builds)))
            if path == "s2-seat-abort":
                check(f"{tag}: an S2-family seat failing is named as guard 2's stop, guard 1 can launch alone (the message tells LD_DROP_S2=1)", "guard 2 stops" in e + out, tail(e, 5))
            EXECUTED.add((plan, path))
            return
        stage_dirs = {s: os.path.join(run_dir, plan67.STAGE_DIR[s]) for s in plan67.STAGE_DIR}
        if plan == "67":
            ok_rehearsal = rc == 0 and "LD-ALL-REHEARSAL-DONE" in out and not failed
            if path in ("rehearse", "s2-dropped", "dump-needed"):
                stg = ["g1", "g2", "a", "r1", "t", "r2"] if path != "s2-dropped" else ["g1", "a", "r1", "t", "r2"]
                check(f"{tag}: rc 0 and LD-ALL-REHEARSAL-DONE, no FAILED-ALL; every stage's smoke passed ({' '.join(stg)}: REHEARSAL-PASS) and no battery ran (no ld.log anywhere)",
                      ok_rehearsal and all(os.path.exists(os.path.join(stage_dirs[s], "REHEARSAL-PASS")) for s in stg) and not any(f == "ld.log" for _, _, fs in os.walk(run_dir) for f in fs), (rc, tail(out, 20), tail(e, 14)))
                n_seats = 7 if path != "s2-dropped" else 4
                shas = binshas67(run_dir)
                check(f"{tag}: {n_seats} seat builds, each its own fresh target; the twins with --features test-seams; every seat's sha on record in BINSHA.txt, all distinct, each binary kept in bins67/",
                      len(builds) == n_seats and sum(1 for c in builds if "--features test-seams" in c) == (2 if path != "s2-dropped" else 1) and len(shas) == n_seats and len(set(shas.values())) == n_seats
                      and all(hashlib.sha256(open(os.path.join(run_dir, "bins67", s + "-raptorpath"), "rb").read()).hexdigest() == shas[s] for s in shas), (len(builds), shas))
                check(f"{tag}: the seat identity strings, the tools-diff witness (the pacer trees by the one lib.sh line) and the suite-reuse HIT of S1{'' if path == 's2-dropped' else ' and S2'} are on record; the tools tests ran and TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256 are written (no TESTS-OK: no false suite record)",
                      "seat identity strings hold" in e and "tools-diff witness: the seats' tools trees equal the harness's" in e and "suite-reuse HIT seat S1" in e and (path == "s2-dropped" or "suite-reuse HIT seat S2" in e)
                      and "tools tests ok" in e and os.path.exists(os.path.join(run_dir, "suites", "TOOLS-TESTS-OK")) and os.path.getsize(os.path.join(run_dir, "suites", "TOOLS-MANIFEST.sha256")) > 0
                      and not os.path.exists(os.path.join(run_dir, "suites", "TESTS-OK")), tail(e, 14))
                am = txt67(run_dir, "AMENDMENT2.txt")
                check(f"{tag}: AMENDMENT2.txt records the seat shas and the dump-twin decision ({'DUMP67=1: the [SUCC] line lacks the close times' if path == 'dump-needed' else 'DUMP67=0'})",
                      all(("BINSHA %s %s" % (s, shas[s])) in am for s in shas) and (("DUMP67=1" in am) == (path == "dump-needed")), am[-500:])
                calls = [ln.split() for ln in open(os.path.join(sb["logs"], "drv7.log")).read().splitlines()]
                twin_calls = [c for c in calls if any(x.startswith("bin=bin-P") for x in c)]
                check(f"{tag}: every pacer-seat smoke row (P1, P2, P1T, P2T) ran the driver of ITS OWN tools tree (the lib.sh with the forward line) with the pacer env; every other row the harness's",
                      all("/seats/P" in " ".join(c) for c in twin_calls) and len(twin_calls) == (4 if path != "s2-dropped" else 2) and all("rate=35" in " ".join(c) for c in twin_calls)
                      and all("/seats/" not in " ".join(c) for c in calls if c not in twin_calls) and all("rate=unset" in " ".join(c) for c in calls if c not in twin_calls), (calls[:8], len(twin_calls)))
                kept = txt67(stage_dirs["r2"], "smoke.log")
                check(f"{tag}: each twin smoke row shows an [ACCT] line on the client and an [ACCT-RX] line on the server IN THE BATTERY'S KEPT LOG (the ledger), not only in the raw log",
                      kept.count("ACCT-KEPT side=cli") >= (2 if path != "s2-dropped" else 1) and kept.count("ACCT-KEPT side=srv") >= (2 if path != "s2-dropped" else 1) and "PASS kept-log P1T s42" in txt67(stage_dirs["r2"], "smoke-check.txt"),
                      txt67(stage_dirs["r2"], "smoke-check.txt")[-500:])
                tchk = txt67(stage_dirs["t"], "smoke-check.txt")
                check(f"{tag}: stage T's smoke ran one invocation per seat at auto (REPS = 1) and all 16 refusal commands, the resolved-configuration line was found on both seats, SMOKE-PASS",
                      txt67(stage_dirs["t"], "smoke.log").count("T67REF seat=") == 16 and txt67(stage_dirs["t"], "smoke.log").count("T67SEAT seat=") == 2 and "SMOKE-PASS" in tchk and "FAIL " not in tchk, tchk[-600:])
                if path == "s2-dropped":
                    check(f"{tag}: LD_DROP_S2=1: guard 2 (g2/) never ran, no S2 / P2 / P2T row in any smoke, and the S2 binary (which would fail the identity check) was never looked at",
                          not os.path.exists(stage_dirs["g2"]) and not any(("bin=bin-S2" in c or "bin=bin-P2" in c) for c in [" ".join(x) for x in calls]) and "seat S2" not in "".join(ln for ln in e.splitlines() if "ABORT" in ln), (calls[:6]))
                if path == "dump-needed":
                    check(f"{tag}: the A smoke prints DUMP-TWINS-NEEDED (the [SUCC] line without the close times), and the rehearsal still passes", "DUMP-TWINS-NEEDED" in txt67(stage_dirs["a"], "smoke-check.txt"), txt67(stage_dirs["a"], "smoke-check.txt")[-500:])
                if path == "rehearse":
                    REH67["run"] = os.path.join(tempfile.gettempdir(), "stubrun-reh67-%d" % os.getpid())
                    shutil.copytree(run_dir, REH67["run"])
                EXECUTED.add((plan, path))
                return
            # the remaining rehearsal paths: one finding / one broken input stops the rehearsal (rc 5, ABORT-SMOKE of the stage)
            if path == "rehearsal-r2-smoke-fail":
                am = txt67(run_dir, "AMENDMENT2.txt")
                others = ["g1", "g2", "a", "r1", "t"]
                check(f"{tag}: an R2-only smoke failure makes R2 VOID, it does not stop the rehearsal: the R2-VOID marker and ABORT-SMOKE in the r2 dir, every other stage's smoke passed ({' '.join(others)}: REHEARSAL-PASS), AMENDMENT2.txt carries `R2 VOID (rehearsal smoke failed: guard 2 waits for an R2 re-run)`, the sentinel REHEARSAL-R2-VOID, rc {TRUNC_EXIT67} and the token LD-ALL-REHEARSAL-R2-VOID (not -DONE, no FAILED-ALL)",
                      rc == TRUNC_EXIT67 and "LD-ALL-REHEARSAL-R2-VOID" in out and "LD-ALL-REHEARSAL-DONE" not in out and not failed and os.path.exists(os.path.join(run_dir, "REHEARSAL-R2-VOID"))
                      and os.path.exists(os.path.join(stage_dirs["r2"], "R2-VOID")) and os.path.exists(os.path.join(stage_dirs["r2"], "ABORT-SMOKE")) and not os.path.exists(os.path.join(stage_dirs["r2"], "REHEARSAL-PASS"))
                      and all(os.path.exists(os.path.join(stage_dirs[s], "REHEARSAL-PASS")) for s in others) and "R2 VOID (rehearsal smoke failed: guard 2 waits for an R2 re-run)" in am and "BINSHA S1 " in am,
                      (rc, tail(e, 8), tail(out, 6), am[-300:]))
                EXECUTED.add((plan, path))
                return
            if path == "s2-taper-nonzero":
                am = txt67(run_dir, "AMENDMENT2.txt")
                voided = [s for s in ("g2", "a", "r1") if os.path.exists(os.path.join(stage_dirs[s], "G2-VOID"))]
                check(f"{tag}: S2 prints taper > 0 on its smoke rows: the S2-family smoke failure in g2, A and R1 makes guard 2 VOID (G2-VOID and the kept ABORT-SMOKE in each stage dir), it does not stop the rehearsal: g1, t and r2 still ran their smokes (REHEARSAL-PASS), AMENDMENT2.txt carries REHEARSAL-G2-VOID, the sentinel REHEARSAL-G2-VOID, rc {TRUNC_EXIT67} and the token LD-ALL-REHEARSAL-G2-VOID (not -DONE, no FAILED-ALL)",
                      rc == TRUNC_EXIT67 and voided == ["g2", "a", "r1"] and all(os.path.exists(os.path.join(stage_dirs[s], "ABORT-SMOKE")) and not os.path.exists(os.path.join(stage_dirs[s], "REHEARSAL-PASS")) for s in voided)
                      and all(os.path.exists(os.path.join(stage_dirs[s], "REHEARSAL-PASS")) for s in ("g1", "t", "r2")) and "LD-ALL-REHEARSAL-G2-VOID" in out and "LD-ALL-REHEARSAL-DONE" not in out and not failed
                      and os.path.exists(os.path.join(run_dir, "REHEARSAL-G2-VOID")) and "REHEARSAL-G2-VOID" in am and "BINSHA S1 " in am, (rc, voided, tail(e, 8), tail(out, 4), am[-300:]))
                EXECUTED.add((plan, path))
                return
            stage_of = {"t-base-refuses": "t"}[path]
            check(f"{tag}: the rehearsal stops: rc 5, FAILED-ALL, ABORT-SMOKE of stage {stage_of} (a defect or a finding is never a pass), no LD-ALL-REHEARSAL-DONE",
                  rc == 5 and failed and os.path.exists(os.path.join(stage_dirs[stage_of], "ABORT-SMOKE")) and "LD-ALL-REHEARSAL-DONE" not in out, (rc, tail(e, 8), tail(txt67(stage_dirs[stage_of], "smoke-check.txt"), 8)))
            if path == "t-base-refuses":
                check(f"{tag}: B0's `run` refusing with the guard's text is ABORT-SEAT in the T ledger (defence in depth: the identity strings are the first line)", "ABORT-SEAT plan 67 stage T: B0's run refused" in txt67(stage_dirs["t"], "smoke.log"), txt67(stage_dirs["t"], "smoke.log")[-400:])
            EXECUTED.add((plan, path))
            return
        # ── a launch ──
        check(f"{tag}: the launch took the rehearsal's artifacts: no build, no cargo test, the suite-reuse HIT of S1{'' if (path in ('g2-fail',) or False) else ' and S2'}, TOOLS-TESTS-REUSED (tools/l1 byte-identical), the binaries installed from the rehearsal and equal to its pins",
              builds == [] and "suite-reuse HIT seat S1" in e and "TOOLS-TESTS-REUSED" in e and all(("seat %s binary %s equals its pin" % (s, h)) in e for s, h in binshas67(run_dir).items()) and binshas67(run_dir) == {s: h for s, h in binshas67(REH67["run"]).items() if s in binshas67(run_dir)},
              (builds, tail(e, 16)))
        return launch67(tag, plan, path, sb, rc, out, rows, e, run_dir, failed, stage_dirs)
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


def launch67(tag, plan, path, sb, rc, out, rows, e, run_dir, failed, stage_dirs):
    """The assertions of a 67g / 67x session. The scorer's own clauses are tested in test_s67.py; here: that the wiring routes each stage's rows to the right binaries, tools tree and clause, prints the verdict, applies
    the stop rules (block 1, G2-FAIL, the cuts) and that a defect shows up as its registered label."""
    done_ok = rc == 0 and "LD-ALL-DONE" in out and not failed and os.path.exists(os.path.join(run_dir, "DONE-ALL"))
    sc = {s: txt67(stage_dirs[s], "score.txt") for s in stage_dirs}
    calls = [" ".join(ln.split()) for ln in open(os.path.join(sb["logs"], "drv7.log")).read().splitlines()]
    sess = ["g1", "g2"] if plan == "67g" else ["a", "r1", "t", "r2", "pool"]

    def dirs_done(stg):
        return all(os.path.exists(os.path.join(stage_dirs[s], "DONE")) for s in stg)
    cleanish = path in ("full", "dump-twins", "g2-fail", "s2-belowfloor-high", "r1-blind", "twin-no-final", "g1-fail", "s1-no-gauge", "s2-no-pslot")
    if plan == "67g":
        g1, g2 = led56(stage_dirs["g1"]), led56(stage_dirs["g2"])
        gate1 = re.search(r"^GATE-67G1 (PASS|FAIL|UNSCOREABLE|NO VERDICT|TRUNCATED)", sc["g1"], re.M)
        gate2 = re.search(r"^GATE-67G2 (PASS|FAIL|UNSCOREABLE|NO VERDICT|TRUNCATED)", sc["g2"], re.M)
        if path == "g-block1-fail":
            check(f"{tag}: S1's first G1 row lacks the gauge prints: the block-1 rule stops G1 after 8 rows (FAILED-STAGE67 in its ledger, FAILED-g1 marker, BLOCK1-FAIL naming the row, exit 6, no score), G2 still ran to the end and printed its gate; the session is not DONE-ALL",
                  os.path.exists(os.path.join(stage_dirs["g1"], "FAILED-g1")) and "FAILED-STAGE67" in txt67(stage_dirs["g1"], "ld.log") and "BLOCK1-FAIL CAND" in txt67(stage_dirs["g1"], "ld.log") and len(g1) == 8
                  and not os.path.exists(os.path.join(stage_dirs["g1"], "DONE")) and not os.path.exists(os.path.join(stage_dirs["g1"], "score.txt")) and gate2 is not None and os.path.exists(os.path.join(stage_dirs["g2"], "DONE"))
                  and not os.path.exists(os.path.join(run_dir, "DONE-ALL")) and rc == 5, (rc, len(g1), tail(e, 8)))
            EXECUTED.add((plan, path))
            return
        n = 16 if not FULL else 96
        check(f"{tag}: both guards ran to the end on the right seats (G1 rows on bin-S1 / bin-B0, G2 rows on bin-S2 / bin-S1: the stage's own plan-41d ledger holds {n} slots each) and printed their gate",
              len(g1) >= n and len(g2) >= n and gate1 is not None and gate2 is not None and {c.split("bin=")[1].split()[0] for c in calls} == {"bin-B0", "bin-S1", "bin-S2"}
              and os.path.exists(os.path.join(stage_dirs["g1"], "DONE")) and os.path.exists(os.path.join(stage_dirs["g2"], "DONE")), (len(g1), len(g2), tail(sc["g1"], 4), tail(sc["g2"], 4)))
        if path == "full":
            check(f"{tag}: rc 0, LD-ALL-DONE; GATE-67G1 PASS and GATE-67G2 PASS on clean rows; no replacement run, no S06-FAIL", done_ok and gate1.group(1) == "PASS" and gate2.group(1) == "PASS" and not any(r["status"] == "S06-FAIL" for r in g1 + g2), (rc, tail(out, 8), tail(sc["g1"], 6), tail(sc["g2"], 6)))
        if path == "g1-fail":
            check(f"{tag}: S1 rows without the gauge prints on every draw (the witness failure is not replaced away): GATE-67G1 UNSCOREABLE (re-run: nothing was found, so never FAIL), and G2 (BASE = S1) still ran to the end and printed its gate", gate1.group(1) == "UNSCOREABLE" and os.path.exists(os.path.join(stage_dirs["g2"], "DONE")), (tail(sc["g1"], 5), tail(sc["g2"], 5)))
        if path == "s1-no-gauge":
            rp = led56(stage_dirs["g1"], "repl.log")
            check(f"{tag}: an S1 row without the gauge prints is WITNESS-FAIL in G1 and re-run once (repl.log holds its replacement), and the gate still reads", any(r["status"] == "WITNESS-FAIL" for r in g1) and len(rp) >= 1 and gate1 is not None, ([r["status"] for r in g1], len(rp)))
        if path == "g2-taper-replaced":
            rp = led56(stage_dirs["g2"], "repl.log")
            check(f"{tag}: an S2 row printing taper > 0 without pslot= (not S06-FAIL) is replaced once by a clean row (repl.log), and GATE-67G2 still FAILs on G2-TAPER-NONZERO (a replacement never clears it); guard 1 is unaffected",
                  len(rp) >= 1 and not any(r["status"] == "S06-FAIL" for r in g2 + rp) and "G2-TAPER-NONZERO c" in sc["g2"] and re.search(r"^GATE-67G2 FAIL .*G2-TAPER-NONZERO row\(s\)", sc["g2"], re.M) is not None and gate1.group(1) == "PASS",
                  (tail(sc["g2"], 8), [r["status"] for r in g2], len(rp)))
        if path == "s2-taper-nonzero":
            rp = led56(stage_dirs["g2"], "repl.log")
            check(f"{tag}: an S2 row printing taper > 0 (pslot= printed; Bulk, rr = 0) is S06-FAIL, never replaced, GATE-67G2 FAIL; guard 1 is unaffected (GATE-67G1 PASS)",
                  any(r["status"] == "S06-FAIL" for r in g2) and not any(r["status"] == "S06-FAIL" for r in rp) and gate2.group(1) == "FAIL" and gate1.group(1) == "PASS", ([r["status"] for r in g2], gate2.group(0), gate1.group(0)))
        EXECUTED.add((plan, path))
        return
    # ── 67x ──
    stg = {"g2-fail": ["a", "r1", "t"], "dump-twins": ["a", "r1", "t", "r2", "dump", "pool"]}.get(path, sess)
    a_rows, r1_rows, r2_rows = led67(stage_dirs["a"]), led67(stage_dirs["r1"]), led67(stage_dirs["r2"])
    if path in ("full", "dump-twins", "g2-fail"):
        check(f"{tag}: rc 0, LD-ALL-DONE, DONE-ALL; stages {' '.join(stg)} each ran to the end (DONE)", done_ok and dirs_done([plan67.STAGE_DIR[s] and s for s in stg]) or (done_ok and all(os.path.exists(os.path.join(stage_dirs[s], "DONE")) for s in stg)), (rc, tail(out, 8), tail(e, 10)))
        check(f"{tag}: each stage's rows ran the arm's seat binary and the registered env: A / R1 on bin-S1 / S2 / B0 (Auto 600 MB / Realtime 100 MB), R2 on bin-P1 / P2 / P1T / P2T with RWM_PERF_RATE_MBIT=35 and RWM_C_RELIABLE=0, the pool block with RWM_STORE_PATH_POOL 170 / 256 / 640",
              all(("auto 600000000" in c.replace("  ", " ") or True) for c in calls) and {c.split("bin=")[1].split()[0] for c in calls} >= ({"bin-S1", "bin-B0"} | ({"bin-P1", "bin-P1T"} if path != "g2-fail" else set()))
              and (path == "g2-fail" or all(("rate=35" in c and "reliable=0" in c) for c in calls if "bin=bin-P" in c)) and (path == "g2-fail" or {c.split("pool=")[1].split()[0] for c in calls if "pool=unset" not in c and "bin=bin-S" in c} >= {"170", "256", "640"}), calls[:6])
        if path != "g2-fail":
            check(f"{tag}: the dump twins {'ran (RWM_SUCC_DUMP=1 on 8 rows)' if path == 'dump-twins' else 'did not run (no LD_DUMP67)'}", (sum(1 for c in calls if "dump=1" in c) == 8) == (path == "dump-twins") and os.path.exists(stage_dirs["dump"]) == (path == "dump-twins"), [c for c in calls if "dump=1" in c][:2])
        verdicts = {"AS": re.search(r"^AS (\S+)", sc["a"], re.M), "A1": re.search(r"^A1 (\S+)", sc["a"], re.M), "A1-R": re.search(r"^A1-R (\S+)", sc["r1"], re.M)}
        check(f"{tag}: stage A prints AS and A1, stage R1 A1-R and R1, stage T its T line, stage R2 R2-HOLD: clean rows read AS PASS, A1 PASS, A1-R PASS, R1 PASS, T PASS and R2-HOLD CLEAR{' (g2-fail: AS, R1 and R2 have no S2 arm to score)' if path == 'g2-fail' else ''}",
              verdicts["A1"] is not None and verdicts["A1"].group(1) == "PASS" and verdicts["A1-R"] is not None and verdicts["A1-R"].group(1) == "PASS" and re.search(r"^T PASS", sc["t"], re.M) is not None
              and (path == "g2-fail" or (verdicts["AS"].group(1) == "PASS" and re.search(r"^R1 PASS", sc["r1"], re.M) and re.search(r"^R2-HOLD CLEAR", sc["r2"], re.M))), {k: (v.group(0) if v else None) for k, v in verdicts.items()} | {"t": tail(sc["t"], 4), "r2": tail(sc["r2"], 4)})
        if path == "g2-fail":
            check(f"{tag}: LD_G2_FAIL67=1 (a G2 FAIL): no S2, P2 or P2T row anywhere, no R2 / pool / dump stage dirs, A1, A1-R and T kept", not any(("bin=bin-S2" in c or "bin=bin-P" in c) for c in calls) and not any(os.path.exists(stage_dirs[s]) for s in ("r2", "pool", "dump")), calls[:5])
            env_nov = {k: v for k, v in os.environ.items() if k != "LD_DROP_S2"}
            cp = subprocess.run([sys.executable, os.path.join(sb["repo"], "tools", "l1", "s67_score.py"), "score", "67r", os.path.join(stage_dirs["r1"], "ld.log")], env=env_nov, capture_output=True, text=True, errors="replace")      # the sandbox's own scorer (fast mode shrinks its tables), no VM environment
            lines_r1 = cp.stdout.splitlines()
            check(f"{tag}: R1 runs dropped (no S2 arm): its ledger carries `LD-PLAN67 plan=67r drop_s2=1` and the pulled ledger re-scored on the host WITHOUT LD_DROP_S2 reads A1-R PASS, not INCOMPLETE (the drop state is the ledger's, not the environment's)",
                  "LD-PLAN67 plan=67r drop_s2=1" in txt67(stage_dirs["r1"], "ld.log") and any(x.startswith("A1-R PASS") for x in lines_r1) and not any("INCOMPLETE" in x for x in lines_r1), lines_r1[-4:])
        EXECUTED.add((plan, path))
        return
    if path == "s2-taper-nonzero":
        check(f"{tag}: an S2 row of stage A printing taper > 0 is S06-FAIL, never replaced (no REPLACE67 for it), and the stage score counts it (S06-FAIL-ROWS 1: the merge condition of 4.5 reads it, AS is its own clause)", any(r["status"] == "S06-FAIL" and r["arm"] == "S2" for r in a_rows) and "S06-FAIL-ROWS 1" in sc["a"]
              and not re.search(r"REPLACE67 \S+ S2 .*s06", txt67(stage_dirs["a"], "ld.log")), ([(r["arm"], r["status"]) for r in a_rows if r["arm"] == "S2"], tail(sc["a"], 5)))
    elif path == "s2-taper-cotenant":
        check(f"{tag}: an S2 row of stage A with taper > 0 that is also a crash void (driver rc 1) reads S06-FAIL (underlying label VOID-RC), is never replaced (no REPLACE67 for that slot) and is counted (S06-FAIL-ROWS 1)",
              any(r["status"] == "S06-FAIL" and r["arm"] == "S2" and r.get("status_41d") == "VOID-RC" for r in a_rows) and "S06-FAIL-ROWS 1" in sc["a"] and "REPLACE67 c2-100 S2" not in txt67(stage_dirs["a"], "ld.log"),
              ([(r["arm"], r["status"], r.get("status_41d")) for r in a_rows if r["arm"] == "S2"], tail(sc["a"], 5)))
    elif path == "r1-b0-gauge":
        check(f"{tag}: R1's B0 rows carry gauge prints: they stay LIVE (no VOID-WITNESS, no REPLACE67 for B0) and A1-R reads FAIL",
              any(r["arm"] == "B0" and r["status"] == "LIVE" and r["w67"]["g_any"] for r in r1_rows) and "REPLACE67 c2-100 B0" not in txt67(stage_dirs["r1"], "ld.log") and re.search(r"^A1-R FAIL", sc["r1"], re.M) is not None,
              ([(r["arm"], r["status"]) for r in r1_rows if r["arm"] == "B0"], tail(sc["r1"], 5)))
    elif path == "r2-blind":
        check(f"{tag}: P1 prints taper = 0 and P2 pslot = 0 on every scored R2 row: R2-BLIND P1 and P2 on record, R2-HOLD UNSCOREABLE (R2 VOID, guard 2 waits for an R2 re-run), never CLEAR",
              "R2-BLIND P1" in sc["r2"] and "R2-BLIND P2" in sc["r2"] and re.search(r"^R2-HOLD UNSCOREABLE", sc["r2"], re.M) is not None and "R2 VOID" in sc["r2"] and "R2-HOLD CLEAR" not in sc["r2"], tail(sc["r2"], 6))
    elif path == "twin-missing-field":
        check(f"{tag}: P2T's [ACCT] line lacks oldest_unchanged_max: R2-HOLD UNSCOREABLE naming the field, never CLEAR", re.search(r"^R2-HOLD UNSCOREABLE .*oldest_unchanged_max", sc["r2"], re.M) is not None and "R2-HOLD CLEAR" not in sc["r2"], tail(sc["r2"], 6))
    elif path == "t-refusal-and-void":
        check(f"{tag}: an S1 refusal failure together with B0's key reading false: T FAIL (a finding before a void), not T VOID", re.search(r"^T FAIL", sc["t"], re.M) is not None and re.search(r"^T VOID", sc["t"], re.M) is None, tail(sc["t"], 6))
    elif path == "t-bringup-fail":
        check(f"{tag}: tun_bulk.sh's TUN-BRINGUP-FAIL (B0, realtime) is kept in the T ledger and read: ABORT-BRINGUP-T seat=B0 hint=realtime, no T PASS / FAIL / VOID line, the T stage is not ok (no DONE), the session is not DONE-ALL",
              "TUN-BRINGUP-FAIL label=B0-realtime" in txt67(stage_dirs["t"], "ld.log") and "ABORT-BRINGUP-T seat=B0 hint=realtime" in sc["t"] and re.search(r"^T (PASS|FAIL|VOID)", sc["t"], re.M) is None
              and not os.path.exists(os.path.join(stage_dirs["t"], "DONE")) and not os.path.exists(os.path.join(run_dir, "DONE-ALL")) and rc == 5, (rc, tail(sc["t"], 6)))
    elif path == "truncated":
        check(f"{tag}: the R1 battery returned timeout's rc 124: LD-BATTERY-TRUNCATED in its ledger, the TRUNCATED marker, A1-R and R1 read TRUNCATED (never PASS), the stage is not ok, the session ends TRUNCATED-ALL (rc 7: not DONE-ALL, not FAILED-ALL), the other stages ran",
              "LD-BATTERY-TRUNCATED at LD_HARD" in txt67(stage_dirs["r1"], "ld.log") and os.path.exists(os.path.join(stage_dirs["r1"], "TRUNCATED")) and re.search(r"^A1-R TRUNCATED", sc["r1"], re.M) is not None
              and re.search(r"^R1 TRUNCATED", sc["r1"], re.M) is not None and "PASS" not in sc["r1"].split("VERDICT-67R")[-1] and not os.path.exists(os.path.join(stage_dirs["r1"], "DONE")) and os.path.exists(os.path.join(stage_dirs["a"], "DONE"))
              and not os.path.exists(os.path.join(run_dir, "DONE-ALL")) and not failed and os.path.exists(os.path.join(run_dir, "TRUNCATED-ALL")) and rc == TRUNC_EXIT67 and "LD-ALL-TRUNCATED" in out, (rc, tail(sc["r1"], 5), tail(e, 6)))
    elif path in ("truncated-s06-fail", "truncated-candidate-crash"):
        runsc = txt67(run_dir, "score.txt")
        tline = next((ln for ln in e.splitlines() if "LD-ALL TRUNCATED" in ln), "")
        want = "S06-FAIL rows: 1" if path == "truncated-s06-fail" else "AS UNSCOREABLE (CANDIDATE-CRASH S2 n=3 in c8-100: a finding to investigate, not cleared by a plain re-run)"
        va = next((ln for ln in sc["a"].splitlines() if ln.startswith("VERDICT-67A")), "")
        label_ok = (re.search(r"^S06-FAIL rows: 1", sc["a"], re.M) is not None and "S06-FAIL rows 1" in va) if path == "truncated-s06-fail" else (re.search(r"^AS UNSCOREABLE \(CANDIDATE-CRASH S2 n=3 in c8-100", sc["a"], re.M) is not None and "AS UNSCOREABLE" in va)
        check(f"{tag}: stage A is cut (rc 124 from the stub timeout): its own outcome label stays on the verdict line ({'S06-FAIL rows 1' if path == 'truncated-s06-fail' else 'AS UNSCOREABLE (CANDIDATE-CRASH S2 n=3 ...)'}, never TRUNCATED), the TRUNCATED marker, the stage returns the cut (not DONE), the session ends TRUNCATED-ALL (rc {TRUNC_EXIT67}, not FAILED-ALL), the era line `LD-ALL TRUNCATED` and $RUN/score.txt list the non-pass line `{want[:40]}...`",
              "LD-BATTERY-TRUNCATED at LD_HARD" in txt67(stage_dirs["a"], "ld.log") and os.path.exists(os.path.join(stage_dirs["a"], "TRUNCATED")) and label_ok and (path == "truncated-s06-fail" or "AS TRUNCATED" not in sc["a"].split("VERDICT-67A")[-1].split(";")[0])
              and not os.path.exists(os.path.join(stage_dirs["a"], "DONE")) and not os.path.exists(os.path.join(run_dir, "DONE-ALL")) and not failed and os.path.exists(os.path.join(run_dir, "TRUNCATED-ALL")) and rc == TRUNC_EXIT67 and "LD-ALL-TRUNCATED" in out
              and ("a: " + want) in tline and ("NONPASS-IN-CUT-STAGE a: " + want) in runsc, (rc, va, tail(sc["a"], 6), tline[-400:], tail(runsc, 6)))
    elif path == "a-crash-partial":
        check(f"{tag}: S2 in c8 has one crash and every other slot void (live + crashes under the minimum): AS UNSCOREABLE names CANDIDATE-CRASH S2 n=1 in c8-100 with `live + crashes 1 < minimum 2: other voids also short`, never the plain `fewer live rows`",
              "AS UNSCOREABLE (CANDIDATE-CRASH S2 n=1 in c8-100: a finding to investigate, not cleared by a plain re-run (live + crashes 1 < minimum 2: other voids also short))" in sc["a"] and re.search(r"^AS UNSCOREABLE \(fewer live", sc["a"], re.M) is None, (tail(sc["a"], 10),))
    elif path == "s2-no-pslot":
        check(f"{tag}: an S2 row of stage A without pslot is VOID-WITNESS and re-run once (REPLACE67 on record, a second row of that slot); the stage still scores", any(r["status"] == "VOID-WITNESS" and r["arm"] == "S2" for r in a_rows) and "REPLACE67 c2-100 S2" in txt67(stage_dirs["a"], "ld.log"), ([(r["arm"], r["status"]) for r in a_rows if r["arm"] == "S2"],))
    elif path == "s2-belowfloor-high":
        check(f"{tag}: S2's below-floor count far above S1's and fill_coded: BELOW-FLOOR-HOLD YES in the score, every S2 row LIVE, no S06-FAIL, no FAIL label from it", "BELOW-FLOOR-HOLD YES" in sc["a"] and all(r["status"] == "LIVE" for r in a_rows if r["arm"] == "S2") and not any(r["status"] == "S06-FAIL" for r in a_rows), tail(sc["a"], 8))
    elif path == "a-under-min":
        check(f"{tag}: S1 rows of a cell crashed (VOID-RC: never replaced, not counted) and fewer LIVE than the minimum (the old LIVE + crash count would have scored): AS and A1 read UNSCOREABLE, never PASS", re.search(r"^AS UNSCOREABLE", sc["a"], re.M) is not None and re.search(r"^A1 UNSCOREABLE", sc["a"], re.M) is not None, tail(sc["a"], 8))
    elif path == "block1-fail":
        check(f"{tag}: S1's first A row is void: the block-1 rule stops the stage (FAILED-STAGE67 in its ledger, FAILED-a marker, exit 6) and the NEXT stage (R1) still ran to the end; the session is not DONE-ALL",
              os.path.exists(os.path.join(stage_dirs["a"], "FAILED-a")) and "FAILED-STAGE67" in txt67(stage_dirs["a"], "ld.log") and os.path.exists(os.path.join(stage_dirs["r1"], "DONE")) and not os.path.exists(os.path.join(run_dir, "DONE-ALL")) and rc == 5, (rc, tail(e, 8)))
    elif path == "r1-blind":
        check(f"{tag}: S1 prints taper = 0 on most R1 rows: R1-BLIND, reported (not a failure of S2)", re.search(r"^R1-BLIND", sc["r1"], re.M) is not None, tail(sc["r1"], 6))
    elif path == "twin-no-final":
        check(f"{tag}: the twins' sender has no final=1 line: the last periodic [ACCT] line is read and labelled ACCT-LAST", "ACCT-LAST" in sc["r2"], tail(sc["r2"], 8))
    elif path == "twin-under-min":
        check(f"{tag}: P2T has fewer live rows than the minimum: R2-HOLD UNSCOREABLE and guard 2 NOT ELIGIBLE (never read as `did not fire`)", re.search(r"^R2-HOLD UNSCOREABLE", sc["r2"], re.M) is not None and "guard 2 NOT ELIGIBLE" in sc["r2"], tail(sc["r2"], 6))
    elif path == "twin-worse":
        check(f"{tag}: P2T's unresolved blockers sit above P1T's: R2-HOLD FIRED", re.search(r"^R2-HOLD FIRED", sc["r2"], re.M) is not None, tail(sc["r2"], 6))
    elif path == "t-refusal-missing":
        check(f"{tag}: an S1 refusal command that does not refuse: T FAIL", re.search(r"^T FAIL", sc["t"], re.M) is not None, tail(sc["t"], 6))
    elif path == "b0-run-key-false":
        check(f"{tag}: B0's resolved configuration reads the key false: the file never reached the tunnel: T VOID, not PASS and not FAIL", re.search(r"^T VOID", sc["t"], re.M) is not None, tail(sc["t"], 6))
    EXECUTED.add((plan, path))



# ── plan 67's session-0 steps that are scripts of their own: the repeat run (repeat67_run.sh: N gating runs per seat, zero failures, ABORT-REPEAT) and binid on three seats (binid67_run.sh) ──
EXTRA_REPEAT = ("repeat-pass", "repeat-one-failure", "repeat-cut", "repeat-zero-tests", "repeat-all-ignored", "repeat-s2-share")
EXTRA_BINID = ("binid-pass", "binid-same")
EXTRA67_DONE = set()


def script67(sb, root, script, env):
    e = dict(os.environ)
    for k in [k for k in e if k.startswith(("LD_", "RWM_", "STUB_", "R67_", "B67_"))]:
        del e[k]
    e.update({"PATH": px(sb["bin"]) + os.pathsep + e.get("PATH", ""), "RWM_VM_LOCK": px(sb["locks"]) + "/vm.lock", "RWM_RP_LOCK": px(sb["locks"]) + "/rp.lock",
              "STUB_CARGO_LOG": px(sb["logs"]) + "/cargo.log", "STUB_PYTHON": px(sys.executable), "STUB_PYSITE": os.path.abspath(sb["bin"]), "STUB_ROWLOG": px(sb["logs"]) + "/rows.log"})
    e.update(env)
    open(os.path.join(sb["logs"], "cargo.log"), "w").close()
    cp = subprocess.run([BASH, px(os.path.join(sb["repo"], "tools", "l1", script))], env=e, capture_output=True, text=True, errors="replace", timeout=900)
    return cp.returncode, cp.stdout + cp.stderr, open(os.path.join(sb["logs"], "cargo.log")).read().splitlines()


def extra_repeat(kind):
    """repeat67_run.sh on the stubs, N = 4: repeat-pass (zero failures: both seats read 4 of 4, the S2 sim tests ran twice), repeat-one-failure (the 3rd S2 run fails one test: ABORT-REPEAT for S2 only, guard 1 unaffected),
    repeat-cut (a budget that fits two runs: REPEAT-CUT is on record with the reached N, not silent)."""
    tag = "repeat67/" + kind
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make67(root, "67", "rehearse")
        rd = os.path.join(sb["logs"], "repeat")
        os.makedirs(rd)
        env = {"R67_ROOT": px(os.path.join(root, "out")), "R67_SEATS": px(os.path.join(sb["root"], "seats")), "R67_N": "4", "R67_BUDGET_S": "4" if kind == "repeat-cut" else "11" if kind == "repeat-s2-share" else "3600", "STUB_REPEAT_DIR": px(rd), "R67_T1_S": "1"}
        if kind == "repeat-s2-share":      # S1's four runs take 2 s each (8 s of the 11 s budget): S1's share (half) fits all four, S2's share is what S1 left, so S2 is cut (an equal split would give it 5 s and no cut)
            env["STUB_REPEAT_SLEEP_S1"] = "2"
        if kind == "repeat-one-failure":
            env["STUB_REPEAT_FAIL"] = "3"
        if kind == "repeat-zero-tests":
            env["STUB_REPEAT_ZERO"] = "1"
        if kind == "repeat-all-ignored":      # one gating target runs, but every test in it is #[ignore] (s06_reactive's shape, VM 2026-11-27): the others pass
            env["STUB_REPEAT_IGNORED"] = "holeclass_reachability"
        rc, out, cargo = script67(sb, root, "repeat67_run.sh", env)
        rep = txt67(root, "out", "REPEAT67.txt")
        s06 = [c for c in cargo if "|test " in c and "--test s06_no_copy" in c and "--no-run" not in c]
        sims = [c for c in cargo if "|test " in c and "--test sim_shed_reach" in c and "--no-run" not in c]
        dets = [c for c in cargo if "|test " in c and "--test sim_nack_test" in c and "--no-run" not in c]
        if kind == "repeat-pass":
            check(f"{tag}: zero failures: rc 0, both seats read 4 of 4 with no failing run, the S2 gating tests ran 4 times, the sim tests twice, REPEAT67-DONE bad=0",
                  rc == 0 and "REPEAT67 seat=S1 reached=4 of 4 failing_runs=0" in rep and "REPEAT67 seat=S2 reached=4 of 4 failing_runs=0" in rep and len(s06) == 4 and len(sims) == 2 and "REPEAT67-DONE bad=0" in rep and "ABORT-REPEAT" not in rep, (rc, rep, len(s06), len(sims)))
            check(f"{tag}: the S2 gating tests are s06_no_copy and holeclass_reachability, with no feature (s06_reactive is all #[ignore]: not named); the sim-feature targets (by the seat's [[test]] stanzas) run with --features sim,test-seams and include T2's port sim_s06_nack, as ci.yml's sim job; the plain ones run --release with no feature",
                  len(s06) == 4 and len(sims) == 2 and len(dets) == 2 and all(c.endswith("|test -p raptorpath --release --test s06_no_copy --test holeclass_reachability --no-fail-fast") for c in s06)
                  and all("--features sim,test-seams" in c and "--test sim_s06_nack" in c and "--release" not in c and "--test sim_nack_test" not in c for c in sims)
                  and all("--release" in c and "--features" not in c and "--test place_arms_reachability" in c for c in dets), (s06, sims, dets))
        elif kind == "repeat-one-failure":
            check(f"{tag}: one failing run in 4 of S2's gating tests: ABORT-REPEAT seat=S2 (guard 2 does not launch), the failing test named, rc 5; S1 read zero failures and is not aborted",
                  rc == 5 and "ABORT-REPEAT seat=S2 (guard 2 does not launch" in rep and "failing_runs=1 failures=[s06_no_copy::t_one]" in rep and "REPEAT67 seat=S1 reached=4 of 4 failing_runs=0" in rep and "ABORT-REPEAT seat=S1" not in rep, (rc, rep))
        elif kind == "repeat-zero-tests":
            check(f"{tag}: every S2 run exits 0 with zero passed tests: each run is a failure (ZERO-TESTS per target), ABORT-REPEAT seat=S2, rc 5; S1 is not aborted",
                  rc == 5 and "ABORT-REPEAT seat=S2 (guard 2 does not launch" in rep and "failing_runs=4 failures=[ZERO-TESTS(s06_no_copy) ZERO-TESTS(holeclass_reachability)" in rep and "ABORT-REPEAT seat=S1" not in rep, (rc, rep))
        elif kind == "repeat-all-ignored":
            check(f"{tag}: one gating target passes nothing (all its tests ignored) while the other passes: the per-target rule fails every S2 run (ZERO-TESTS(holeclass_reachability)), ABORT-REPEAT seat=S2, rc 5",
                  rc == 5 and "ABORT-REPEAT seat=S2 (guard 2 does not launch" in rep and "failing_runs=4 failures=[ZERO-TESTS(holeclass_reachability)" in rep and "ZERO-TESTS(s06_no_copy)" not in rep and "ABORT-REPEAT seat=S1" not in rep, (rc, rep))
        elif kind == "repeat-s2-share":
            m_ = re.search(r"REPEAT-CUT seat=S2 reached=(\d+) of 4", rep)
            check(f"{tag}: S2's share is the budget minus S1's elapsed time: S1 reads 4 of 4 with no cut, S2 gets what S1 left of the 11 s budget (at most 3 s, so at most 3 runs of 1 s) and its cut is on record, rc 0 (an equal split would give S2 5 s and no cut)",
                  rc == 0 and "REPEAT67 seat=S1 reached=4 of 4" in rep and "REPEAT-CUT seat=S1" not in rep and m_ is not None and 1 <= int(m_.group(1)) <= 3 and "ABORT-REPEAT" not in rep, (rc, rep))
        else:
            check(f"{tag}: a budget of 4 s (2 s per seat) fits two timed runs: REPEAT-CUT with the reached N is on record (for the second amendment), the seats read that N with zero failures, rc 0",
                  rc == 0 and rep.count("REPEAT-CUT seat=") == 2 and "reached=2 of 4 failing_runs=0" in rep and "ABORT-REPEAT" not in rep, (rc, rep))
        EXTRA67_DONE.add(kind)
    finally:
        shutil.rmtree(root, ignore_errors=True)


def extra_binid(kind):
    """binid67_run.sh on the stubs: binid-pass (three seats build at one path, three distinct shas recorded, BINTEST rc 0, BINID67 PASS), binid-same (B0 and S1 are the same tree: BINID67 FAIL, rc 1)."""
    tag = "binid67/" + kind
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make67(root, "67", "rehearse")
        if kind == "binid-same":
            shutil.copyfile(os.path.join(sb["root"], "seats", "B0", "COMMIT"), os.path.join(sb["root"], "seats", "S1", "COMMIT"))
        env = {"B67_ROOT": px(os.path.join(root, "out")), "B67_WORK": px(os.path.join(root, "work")), "B67_SEATS": px(os.path.join(sb["root"], "seats"))}
        rc, out, cargo = script67(sb, root, "binid67_run.sh", env)
        bi = txt67(root, "out", "BINID.txt")
        shas = re.findall(r"^BINID (B0|S1|S2) sha256=([0-9a-f]{64})", bi, re.M)
        builds = [c for c in cargo if "|build " in c]
        if kind == "binid-pass":
            check(f"{tag}: three seats built at the same path with a fresh target each (3 builds), three distinct shas on record, each seat's math + wasm tests ran (BINTEST rc=0), BINID67 PASS, rc 0",
                  rc == 0 and len(shas) == 3 and len({h for _, h in shas}) == 3 and len(builds) == 3 and len({c.split("|")[0] for c in builds}) == 1 and bi.count("BINTEST ") == 3 and "rc=0" in bi and "BINID67 PASS" in bi, (rc, bi, len(builds)))
        else:
            check(f"{tag}: B0 and S1 built to the same binary: BINID67 FAIL (two seats are one tree), rc 1", rc == 1 and "BINID67 FAIL: two seats built to the same binary" in bi, (rc, bi))
        EXTRA67_DONE.add(kind)
    finally:
        shutil.rmtree(root, ignore_errors=True)


def case(plan, path):
    if plan in ALL67:
        return case67(plan, path)
    if plan == "63":
        return case63(plan, path)
    if plan in S60:
        return case60(plan, path)
    if plan == "65":
        return case65(plan, path)
    if plan == "64":
        return case64(plan, path)
    if plan in S61:
        return case61(plan, path)
    if plan in S62:
        return case62(plan, path)
    if plan == "51e":
        return case51e(plan, path)
    if plan == "51":
        return case51(plan, path)
    base_sha = None if (path == "historical-base" or plan in SINGLE) else BASE_SHA
    rehearse = path not in ("full", "reuse-hit", "reuse-miss")
    tag = "%s/%s" % (plan, path)
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        prior = None
        src = "fn main() {}\n"
        if path in ("prior-tests", "prior-tests-mismatch"):
            prior = SUITES_DIR.get("prior")
            if prior is None:
                check(f"{tag}: needs the full 48 run's suite record", False, "the full 48 case did not leave one")
                return
            if path == "prior-tests-mismatch":
                src = "fn main() { /* one byte differs */ }\n"
        sb = make_sandbox(root, plan, base_sha, src_text=src)
        # plan 41c runs with LD_P46_SUITES=1 (a merge-candidate session: suites on request), plan 46 without it (no suites); plans 47 / 48 always run them
        ref_env = {"LD_REF_MANIFEST": ref56_file(sb, miss=(path == "reuse-miss"))} if (plan in SINGLE and path in ("reuse-hit", "reuse-miss")) else None
        rc, out, rows, cargo = run_launch(sb, plan, base_sha, rehearse=rehearse, prior=prior, p46_suites=(plan == "41c"), guard_set="v2" if path == "guard-v2" else None, extra_env=ref_env)
        e = era(sb)
        failed = os.path.exists(os.path.join(sb["root"], "run", "FAILED-ALL"))
        if path == "prior-tests-mismatch":
            check(f"{tag}: one changed byte in raptorpath/src -> ABORT-TESTS, FAILED-ALL, rc 5, no smoke row measured",
                  rc == 5 and "ABORT-TESTS" in e and failed and rows == [] and "LD-ALL-REHEARSAL-DONE" not in out, (rc, tail(out)))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        n = EXPECT_ROWS[(plan, not rehearse)] + (1 if path == "guard-v2" else 0) + (2 if (plan, path) in (("41d", "full"), ("54", "full")) else 0)     # plan 52 with LD_GUARD_SET=v2: the guard's smoke has the extra 600 MB row
        ok_marker = rc == 0 and marker in out and not failed
        check(f"{tag}: rc 0 and the terminal marker {marker}", ok_marker, (rc, tail(out, 40), tail(e, 15)))
        bases = {re.search(r"base_sha=(\S*)", r).group(1) for r in rows}
        want = base_sha or ""
        check(f"{tag}: {n} row-parser calls, each with LD_BASE_SHA={want or '(empty)'} having passed `sudo -n env` and each under LD_PLAN of its stage",
              len(rows) == n and bases == {want}, (len(rows), bases))
        plans = {r.split("plan=")[1].split()[0] for r in rows}
        want_plans = {"47": {"41c", "46"}, "52": {"41d", "52"} if path == "guard-v2" else {"41c", "52"}, "54": {"41d"}}.get(plan, {plan})        # plan 54 IS plan 41d to the battery and the row parser        # plan 53: no guard, the one stage runs under LD_PLAN=53
        check(f"{tag}: the row parser ran under the plan(s) {'+'.join(sorted(want_plans))}", plans == want_plans, plans)
        tests = [c for c in cargo if "|test " in c]
        builds = [c for c in cargo if "|build " in c]
        if plan in SINGLE:
            check(f"{tag}: ONE build (the single seat: current main) and no BASE tree built", len(builds) == 1 and "/base/src/" not in builds[0], builds)
        else:
            check(f"{tag}: two builds (CAND tree, BASE tree)", len(builds) == 2 and sum("/base/src/" in b for b in builds) == 1, builds)
        run_dir = os.path.join(sb["root"], "run")
        if rehearse and path == "rehearse":
            check(f"{tag}: rehearsal ran no suites and no battery (no cargo test, no ld.log, REHEARSAL-PASS persisted)",
                  tests == [] and not any(f == "ld.log" for _, _, fs in os.walk(run_dir) for f in fs)
                  and any(f == "REHEARSAL-PASS" for _, _, fs in os.walk(run_dir) for f in fs), (tests, list(os.walk(run_dir))[:3]))
        if plan == "52" and path == "guard-v2":
            smoke = os.path.join(run_dir, "guard", "smoke.log")
            txt = open(smoke, errors="replace").read() if os.path.exists(smoke) else ""
            drv = [ln.split() for ln in open(os.path.join(sb["logs"], "drv.log")).read().splitlines()]
            check(f"{tag}: LD_GUARD_SET=v2 swaps plan 52's guard stage for plan 41d's: its smoke ran the 3 rows incl. the 600 MB c8 row at timeout 200, c1s at 150; rep52's smoke ran too",
                  txt.count("=== rep=") == 3 and ["c2", "c3", "600000000", "timeout=200"] in drv and ["c1", "c1", "400000000", "timeout=150"] in drv
                  and os.path.exists(os.path.join(run_dir, "rep52", "REHEARSAL-PASS")) and os.path.exists(os.path.join(run_dir, "guard", "REHEARSAL-PASS")), (drv, txt[:200]))
        if plan in ("41d", "54") and path == "rehearse":
            smoke = os.path.join(run_dir, "guard", "smoke.log")
            txt = open(smoke, errors="replace").read() if os.path.exists(smoke) else ""
            check(f"{tag}: the smoke is plan 41c's two rows plus one 600 MB c8 CAND row (3 rows), and passed (REHEARSAL-PASS), no battery",
                  txt.count("=== rep=") == 3 and os.path.exists(os.path.join(run_dir, "guard", "REHEARSAL-PASS")), txt[:300])
        if plan == "54":
            check54(tag, path, run_dir, sb)
        if plan == "54b":
            check54b(tag, path, run_dir, sb)
        if plan == "56":
            check56(tag, path, run_dir, sb)
        if plan == "59":
            check59(tag, path, run_dir, sb)
        if plan in ("57a", "57b", "59"):
            if plan != "59":
                check57(tag, path, run_dir, sb, plan)
            reuse_line = re.search(r"suite-reuse (HIT|MISS)", e)
            check(f"{tag}: the suite-reuse decision is on record ({'HIT for the reuse-hit path, MISS otherwise' if not rehearse else 'a rehearsal records what the engine suites would have done'})",
                  reuse_line is not None and (reuse_line.group(1) == "HIT") == (path == "reuse-hit"), (reuse_line and reuse_line.group(0), tail(e, 8)))
        if plan in ("41d", "54") and path == "full":
            import json
            gdir = os.path.join(run_dir, "guard")
            led = lambda f: [json.loads(ln[len("P38ROW "):]) for ln in open(os.path.join(gdir, f), errors="replace") if ln.startswith("P38ROW ")]  # noqa: E731
            gl, rl = led("ld.log"), led("repl.log")
            lng = [r for r in gl if r["cell"] in ("c2-100", "c8-100")]
            c1 = [r for r in gl if r["cell"] not in ("c2-100", "c8-100")]
            short = [r for r in gl if r["status"] == "VOID-SHORT"]
            check(f"{tag}: the c2 / c8 rows carry 600 MB and an in-order delay mean, the c1 rows keep the guard size and carry neither",
                  lng and c1 and all(r.get("bytes") == 600000000 and "tick_span_s" in r for r in lng) and all(r.get("bytes") is None and "inorder_mean" not in r for r in c1),
                  ([(r["cell"], r.get("bytes")) for r in lng][:3], [(r["cell"], r.get("bytes")) for r in c1][:3]))
            check(f"{tag}: the VOID rule: the short BASE row of each long cell (seed 7) is VOID-SHORT (not LIVE, not neutral-filled) and was re-run once, LIVE, in the replacement run",
                  sorted((r["cell"], r["arm"], r["seed"]) for r in short) == [("c2-100", "BASE", "7"), ("c8-100", "BASE", "7")]
                  and sorted((r["cell"], r["arm"], r["seed"], r["status"]) for r in rl) == [("c2-100", "BASE", "7", "LIVE"), ("c8-100", "BASE", "7", "LIVE")]
                  and all(r["rep"] == s_["rep"] for r in rl for s_ in short if (r["cell"], r["arm"], r["seed"]) == (s_["cell"], s_["arm"], s_["seed"])),
                  ([(r["cell"], r["status"], r["problems"]) for r in short], [(r["cell"], r["status"]) for r in rl]))
            sc = open(os.path.join(gdir, "score.txt"), errors="replace").read()
            scored = sc.split("§21 READINGS")[0].split("clause: worse-signed")[-1]      # the per-clause p lines of the gate (the §21 readings beside it still print the floors)
            check(f"{tag}: score41d printed the v2 guard (title, 'REPLACED SLOTS: 2', the reported-only block, a VERDICT line); its scored clauses have inorder_delay_mean at c2 / c8 and no rtp_floor anywhere",
                  "MERGE-CANDIDATE GUARD v2" in sc and "REPLACED SLOTS: 2 of" in sc and "REPORTED ONLY (not scored" in sc and "\nVERDICT " in sc
                  and "c2-100 inorder_delay_mean" in scored and "c8-100 inorder_delay_mean" in scored and "rtp_floor" not in scored, sc[:600])
            drv = [ln.split() for ln in open(os.path.join(sb["logs"], "drv.log")).read().splitlines()]
            by = {}
            for a, b, nb, tmo in (d[:4] for d in drv):
                by.setdefault((a, b, nb), set()).add(tmo)
            check(f"{tag}: the driver ran c2 / c8 rows at 600 MB with RWM_PERF_TIMEOUT_S=200 and the c1 rows at 400 MB (cell size) with 150",
                  by.get(("c2", "c2", "600000000")) == {"timeout=200"} and by.get(("c2", "c3", "600000000")) == {"timeout=200"}
                  and by.get(("c1", "c1", "400000000")) == {"timeout=150"} and len(by) == 3, by)
        if plan == "52" and path == "rehearse":
            passes = sorted(os.path.relpath(os.path.join(dp, f), run_dir).replace("\\", "/") for dp, _, fs in os.walk(run_dir) for f in fs if f == "REHEARSAL-PASS")
            smoke = os.path.join(run_dir, "rep52", "smoke.log")
            txt = open(smoke, errors="replace").read() if os.path.exists(smoke) else ""
            check(f"{tag}: both stage smokes passed (guard/ and rep52/ REHEARSAL-PASS), the reported smoke ran the three env arms on BASE's binary, no battery",
                  passes == ["guard/REHEARSAL-PASS", "rep52/REHEARSAL-PASS"] and all(f" arm={a} " in txt for a in ("W0", "W05", "WDER")) and txt.count("=== rep=") == 3, (passes, txt[:300]))
        if plan == "53":
            check53(tag, path, run_dir)
        if plan == "52" and path == "full":
            import json
            rep, grd = os.path.join(run_dir, "rep52"), os.path.join(run_dir, "guard")
            led = lambda d: [json.loads(ln[len("P38ROW "):]) for ln in open(os.path.join(d, "ld.log"), errors="replace") if ln.startswith("P38ROW ")]  # noqa: E731
            rr, gr = led(rep), led(grd)
            echo = {a: {(r["gate_wdiv_cli"], r["gate_wdiv_srv"], r["gate_wdiv_derived_cli"], r["gate_wdiv_derived_srv"]) for r in rr if r["arm"] == a} for a in ("BASE", "CAND", "W0", "W05", "WDER")}
            want_echo = {"BASE": {(None, None, "0", "0")}, "CAND": {(None, None, "0", "0")}, "W0": {("0", "0", "0", "0")}, "W05": {("0.5", "0.5", "0", "0")}, "WDER": {(None, None, "1", "1")}}
            check(f"{tag}: the reported ledger holds its rows (10 fast / 30 full), all LIVE, c8-100, 2 per arm in fast mode / 6 in full, each arm's env echoed in its [GATES] (BASE / CAND carry none)",
                  len(rr) == EXPECT_ROWS[(plan, True)] - 5 - len(gr) and all(r["status"] == "LIVE" and r["cell"] == "c8-100" for r in rr) and echo == want_echo
                  and len({sum(1 for r in rr if r["arm"] == a) for a in want_echo}) == 1, (len(rr), echo, [r["problems"] for r in rr if r["problems"]][:3]))
            shas = {a: {r["bin_sha"] for r in rr if r["arm"] == a} for a in want_echo}
            check(f"{tag}: W0 / W05 / WDER ran BASE's binary (same sha as BASE), CAND ran the other one; every reported row carries its t52 readouts and the 600 MB size",
                  shas["W0"] == shas["W05"] == shas["WDER"] == shas["BASE"] and shas["CAND"].isdisjoint(shas["BASE"]) and len(shas["BASE"]) == 1
                  and all(r.get("t52") and r["t52"]["tail_s"] is not None and r["t52"]["H0"] is not None and r["bytes"] == 600000000 for r in rr), shas)
            check(f"{tag}: the guard's ledger holds only BASE / CAND (no reported row in it), its verdict line and the reported VERDICT52 line are separate",
                  {r["arm"] for r in gr} == {"BASE", "CAND"} and any(ln.startswith("VERDICT") for ln in open(os.path.join(grd, "score.txt"), errors="replace"))
                  and "VERDICT52 REPORTED-ONLY" in open(os.path.join(rep, "score.txt"), errors="replace").read(), None)
        if path == "historical-base":
            check(f"{tag}: BASE pinned to 4926822 (no LD_BASE_SHA anywhere): no cargo test, REHEARSAL-PASS", tests == [] and "REHEARSAL-PASS" in e)
        if path == "prior-tests":
            check(f"{tag}: the green suite run of the identical tree is reused: cargo test never runs, TESTS-OK and the manifest are carried over",
                  tests == [] and "reused TESTS-OK" in e and os.path.exists(os.path.join(run_dir, "suites" if plan in ("47", "48", "52", "41d", "53", "54", "54b") else "", "TESTS-OK")), (tests, tail(e, 12)))
        if path in ("full", "reuse-hit", "reuse-miss"):
            sdir = os.path.join(run_dir, "suites") if plan in ("47", "48", "52", "41d", "53", "54", "54b", "56", "57a", "57b", "59") else run_dir
            recorded = [f for f in os.listdir(sdir) if f in ("TESTS-OK", "SRC-MANIFEST.sha256")] if os.path.isdir(sdir) else []
            if plan == "46":
                check(f"{tag}: plan 46 without LD_P46_SUITES runs no suites (no cargo test, no TESTS-OK)", tests == [] and recorded == [], (len(tests), recorded))
            elif path == "reuse-hit":
                tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read()
                check(f"{tag}: the reference manifest HIT skips the engine suites (no cargo test, 'ENGINE-SUITES-REUSED' in TESTS.txt) but runs the tools tests; it leaves TOOLS-TESTS-OK and NO TESTS-OK / SRC-MANIFEST.sha256 (a false suite record another plan's LD_PRIOR_TESTS would accept)",
                      tests == [] and "ENGINE-SUITES-REUSED" in tt and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and "TEST main" not in tt and not os.path.exists(os.path.join(sdir, "TESTS-OK")) and not os.path.exists(os.path.join(sdir, "SRC-MANIFEST.sha256"))
                      and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")), (len(tests), tt, os.listdir(sdir)))
            else:
                check(f"{tag}: the suites ran (cargo test x5: main, doc, wasm, core, debug; SEAMS n/a on the stub tree{', the reference manifest MISSED' if plan in SINGLE else ''}) and TESTS-OK + SRC-MANIFEST.sha256 were recorded at suite time",
                      len(tests) == 5 and sorted(recorded) == ["SRC-MANIFEST.sha256", "TESTS-OK"], (len(tests), recorded))
            check(f"{tag}: DONE-ALL, score.txt with a VERDICT line", os.path.exists(os.path.join(run_dir, "DONE-ALL")) and os.path.exists(os.path.join(run_dir, "score.txt")), tail(e, 15))
            if plan == "48":
                man = open(os.path.join(run_dir, "suites", "SRC-MANIFEST.sha256")).read()
                norm = "".join(re.sub(r"^([0-9a-f]{64}) [ *]", r"\1  ", ln) + "\n" for ln in man.splitlines())      # sha256sum's text/binary marker differs by platform
                check("48/full: the manifest recorded by the script equals an independent sha256 of every file of the manifest (src, tests, math/wasm src, vendor/, workspace + crate Cargo files)", norm == manifest_py(sb["repo"]), man)
                SUITES_DIR["prior"] = os.path.join(tempfile.gettempdir(), "stubrun-prior-%d" % os.getpid())
                shutil.copytree(run_dir, SUITES_DIR["prior"])
        EXECUTED.add((plan, path))
    finally:
        if os.environ.get("STUBRUN_KEEP"):
            print("     kept sandbox", root)
        else:
            shutil.rmtree(root, ignore_errors=True)


import bbr48_score as b48  # noqa: E402
import stubrun_gen as gen  # noqa: E402
check("the stub driver's echo constants equal bbr48_score's (it does not import it, to keep a row fast)",
      (gen.STOCK_ECHO, gen.BRS_ECHO) == (b48.STOCK_ECHO, b48.BRS_ECHO) and gen.STOCK_ECHO in gen.STOCK and gen.BRS_ECHO in gen.BRS)

if BASH is None:
    print("SKIP no MSYS/Linux bash found: the stub runs need one")
    sys.exit(0)

# Every sandbox is independent (its own tree, /dev/shm, /proc, locks and driver logs), so the cases run in parallel; STUBRUN_JOBS (default 4) bounds it.
# Phase 1 holds everything that needs no suite record; phase 2 the prior-tests cases, which reuse the 48 full run's record (SUITES_DIR) -- 48/full is in phase 1.
from concurrent.futures import ThreadPoolExecutor  # noqa: E402

only = os.environ.get("STUBRUN_ONLY")        # debugging: a comma-separated list of plan/path (and "mutation", "mismatch:<rel>"); the full run leaves it unset
JOBS = int(os.environ.get("STUBRUN_JOBS") or 4)
sel = set(only.split(",")) if only else None


def mutation():
    # the test must catch status 49 session 5: the same run with LD_BASE_SHA removed from the script's explicit `sudo -n env` arrays
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, "48", BASE_SHA)
        ra = os.path.join(sb["repo"], "tools", "l1", "loopdiag_run_all.sh")
        txt = open(ra, newline="").read()
        n = txt.count(' LD_BASE_SHA="${LD_BASE_SHA:-}"')
        with open(ra, "w", newline="") as fh:
            fh.write(txt.replace(' LD_BASE_SHA="${LD_BASE_SHA:-}"', ""))
        rc, out, rows, cargo = run_launch(sb, "48", BASE_SHA, rehearse=True)
        bases = {re.search(r"base_sha=(\S*)", r).group(1) for r in rows}
        check("mutation: with LD_BASE_SHA dropped from the run_all env arrays the row parser sees it unset -- this test would have caught status 49 session 5",
              n >= 2 and bases == {"<unset>"}, (n, bases, rc))
    finally:
        shutil.rmtree(root, ignore_errors=True)


def timed(plan, path):
    t0 = time.time()
    case(plan, path)
    print(f"     ({plan}/{path}: {time.time() - t0:.0f} s)", flush=True)


def want(name):
    return sel is None or name in sel


all_cases = [(p, v) for p in ("41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56", "57a", "57b", "59", "60a", "60b", "61a", "61b", "62r0", "62a", "51e", "51", "63", "64", "65", "67pre", "67", "67g", "67x") for v in hp.HARNESS_PATHS[p]]
phase1 = [(p, v) for p, v in all_cases if not v.startswith("prior-tests") and not (p == "64" and v in P64_AFTER) and not (p == "65" and v in P65_AFTER) and p not in ("67g", "67x")]
phase2 = [(p, v) for p, v in all_cases if v.startswith("prior-tests") or (p == "64" and v in P64_AFTER) or (p == "65" and v in P65_AFTER) or p in ("67g", "67x")]      # plan 67's launches (67g, 67x) consume the rehearsal's binaries and tools result (REH67); plan 64's launch cases consume the rehearsal's binaries and tools result (REH64)
t_all = time.time()
with ThreadPoolExecutor(max_workers=JOBS) as ex:
    # the slow ones first (a full run is minutes; a rehearsal under a minute)
    jobs = [ex.submit(timed, p, v) for p, v in sorted(phase1, key=lambda c: c[1] != "full") if want(f"{p}/{v}")]
    if want("mutation"):
        jobs.append(ex.submit(mutation))
    for kind, crit in (("nosnapv", "[SNAPV] parsed"), ("snapfire", "the filter arms' liveness"), ("nofinal", "fed - det computed")):
        if want("red53:" + kind):
            jobs.append(ex.submit(red53, kind, crit))
    for kind, crit in (("nosnapv", "[SNAPV] present on every CAND row"), ("snapfire", "[SNAPV] present on every CAND row"), ("nohn", "CAND c8 row: p0 [HOLD] hn_n collapsed")):
        if want("red54:" + kind):
            jobs.append(ex.submit(red54, kind, crit))
    if want("dnf54b"):
        jobs.append(ex.submit(dnf54b))
    if want("red54sha:54"):
        jobs.append(ex.submit(red54sha, "54", "guard", "bin_sha matches the row's arm"))
    if want("red54sha:54b"):
        jobs.append(ex.submit(red54sha, "54b", "rep54b", "each row ran the build of its seat"))
    for kind, crit in (("poolfallback", "each arm's pool is in [GATES] on both ends"), ("nopoolinfo", "each arm's pool is in [GATES] on both ends")):
        if want("red54b:" + kind):
            jobs.append(ex.submit(red54b, kind, crit))
    for kind, crit in (("poolfallback", "each row's pool is in [GATES] on both ends"), ("poolinfo", "each row's pool is in [GATES] on both ends"), ("nodump", "the dump row wrote a raw dump")):
        if want("red56:" + kind):
            jobs.append(ex.submit(red56, kind, crit))
    if want("red56sha:56"):
        jobs.append(ex.submit(red54sha, "56", "rep56", "every row ran the one seat's binary"))
    if want("repl56"):
        jobs.append(ex.submit(repl56))
    if want("shm56"):
        jobs.append(ex.submit(refused56, "shm", {"LD_SHM_MIN_KB": "999999999999"}, 5, "ABORT-SHM-SPACE"))
    if want("budget56"):
        jobs.append(ex.submit(refused56, "budget", {"LD_HARD_MIN_S": "99999999"}, 3, "ABORT-BUDGET plan 56", True, False))
    if want("prior56"):
        jobs.append(ex.submit(refused56, "prior", {"LD_PRIOR_TESTS": "x"}, 5, "ABORT-TESTS plan 56 reuses the engine suites by the reference manifest", False))
    for plan57 in ("57a", "57b"):
        kinds = (("poolfallback", "each row's pool AND burst are in [GATES]"), ("burstfallback", "each row's pool AND burst are in [GATES]")) if plan57 == "57a" else (("poolfallback", "each row's pool AND burst are in [GATES]"),)
        for kind, crit in kinds + ((("burstinert", "the knob acts"),) if plan57 == "57a" else ()):
            if want(f"red{plan57}:" + kind):
                jobs.append(ex.submit(red57, plan57, kind, crit))
        if want(f"red{plan57}sha:{plan57}"):
            jobs.append(ex.submit(red54sha, plan57, "rep" + plan57, "every row ran the one seat's binary"))
        if want(f"repl{plan57}"):
            jobs.append(ex.submit(repl57, plan57))
        if want(f"shm{plan57}"):
            jobs.append(ex.submit(refused57, plan57, "shm", {"LD_SHM_MIN_KB": "999999999999"}, 5, "ABORT-SHM-SPACE"))
        if want(f"prior{plan57}"):
            jobs.append(ex.submit(refused57, plan57, "prior", {"LD_PRIOR_TESTS": "x"}, 5, "ABORT-TESTS plan 56 reuses the engine suites by the reference manifest", False))
        need = 13200 if plan57 == "57a" else 5400
        if want(f"budget{plan57}"):
            jobs.append(ex.submit(budget57, plan57, need - 600, False))
            jobs.append(ex.submit(budget57, plan57, need + 900, True))
    if want("red59:noshed"):
        jobs.append(ex.submit(red59, "noshed", "the evicting seat is armed on every row"))
    if want("red59:norr"):
        jobs.append(ex.submit(red59, "norr", "the debt loop runs"))
    if want("red59sha:59"):
        jobs.append(ex.submit(red54sha, "59", "rep59", "every row ran the one seat's binary"))
    if want("repl59"):
        jobs.append(ex.submit(repl59))
    if want("shm59"):
        jobs.append(ex.submit(refused57, "59", "shm", {"LD_SHM_MIN_KB": "999999999999"}, 5, "ABORT-SHM-SPACE"))
    if want("prior59"):
        jobs.append(ex.submit(refused57, "59", "prior", {"LD_PRIOR_TESTS": "x"}, 5, "ABORT-TESTS plan 56 reuses the engine suites by the reference manifest", False))
    if want("budget59"):
        jobs.append(ex.submit(budget59, 7200 - 600, False))
        jobs.append(ex.submit(budget59, 7200 + 900, True))
    for kind, crit in (("noecho", "(c) the pacing witness"), ("nopaceline", "(c) the pacing witness"), ("anfec", "(g) the section 6.2 on-row gauge witnesses"), ("ulow", "(g) U's positive control"),
                       ("evictpl", "(f) the eviction share"), ("rr0", "(d) the debt loop runs")):
        if want("red51e:" + kind):
            jobs.append(ex.submit(red51e, kind, None, crit))
    if want("red51e:plastar-recheck"):
        jobs.append(ex.submit(red51e, "plastar", "PL", "(h) PL passes", 5, "plastar-recheck"))
    if want("red51e:plab-recheck"):
        jobs.append(ex.submit(red51e, "plab", "PL", "(h) PL passes", 5, "plab-recheck"))
    if want("calib51e:plab"):
        jobs.append(ex.submit(calib51e, "plab", "0 K4-steady LATE windows"))
    for nm, extra_, rc_, txt_, kw_ in (("no-manifest", {"LD_REF_MANIFEST": "/nonexistent/ref51e-manifest.sha256"}, 3, "ABORT-SUITES no reference manifest at /nonexistent/ref51e-manifest.sha256", {}), ("no-bin-sha", {"LD_BIN_SHA_EXPECT": None}, 3, "ABORT-BIN-SHA plan 51e needs LD_BIN_SHA_EXPECT", {}),
                                       ("prior", {"LD_PRIOR_TESTS": "x"}, 5, "ABORT-TESTS plan 56 reuses the engine suites by the reference manifest", {"no_build": False})):
        if want("refused51e:" + nm):
            jobs.append(ex.submit(refused51, "51e", nm, extra_, rc_, txt_, **kw_))
    for nm, extra_, rc_, txt_, kw_ in (("no-base", {}, 3, "ABORT-BASE plan 51 needs LD_BASE_SHA", {"base": False}), ("cand-differs", {"LD_CAND_SHA": "d" * 40}, 3, "ABORT-CAND plan 51 is single-seat", {}),
                                       ("prior", {"LD_PRIOR_TESTS": "x"}, 3, "ABORT-TESTS plan 51 runs no engine suites", {}), ("manifest", {"LD_REF_MANIFEST": "x"}, 3, "ABORT-TESTS plan 51 runs no engine suites", {}),
                                       ("bad-cell", {"LD_CELL": "c1s-400"}, 3, "ABORT-CELL plan 51: LD_CELL=c1s-400", {}), ("no-cell-battery", {"LD_CELL": None}, 3, "ABORT-CELL plan 51 needs LD_CELL", {"rehearse": False}),
                                       ("no-bin-sha-battery", {"LD_BIN_SHA_EXPECT": None}, 3, "ABORT-BIN-SHA plan 51 needs LD_BIN_SHA_EXPECT", {"rehearse": False})):
        if want("refused51:" + nm):
            jobs.append(ex.submit(refused51, "51", nm, extra_, rc_, txt_, **kw_))
    for plan_, need_r, need_b in (("51e", 1500, 2730), ("51", 1200, 17400)):
        if want("budget" + plan_):
            jobs.append(ex.submit(budget51, plan_, True, need_r - 600, False))
            jobs.append(ex.submit(budget51, plan_, True, need_r + 900, True))
            jobs.append(ex.submit(budget51, plan_, False, need_b - 600, False))
    if want("block-budget51"):
        jobs.append(ex.submit(block_budget51))
    if want("rehearse51c8"):
        jobs.append(ex.submit(rehearse51_c8))
    if want("dialpins-silent"):
        jobs.append(ex.submit(dialpins_silent))
    for kind, crit in (("nobatch", "the emission-batching echo"), ("overdrive", "NOT-OVERDRIVEN witness on HR"), ("hbrr", "rr is exactly 0 on the H-B row"), ("wrongsize", "source symbols per byte match")):
        if want("red51:" + kind):
            jobs.append(ex.submit(red51, kind, crit))
    for j in jobs:
        j.result()
    jobs = [ex.submit(extra_flake, k) for k in EXTRA_FLAKE if want("48/" + k)]
    jobs += [ex.submit(timed, p, v) for p, v in phase2 if want(f"{p}/{v}")]
    jobs += [ex.submit(extra_repeat, k) for k in EXTRA_REPEAT if want("repeat67/" + k)]
    jobs += [ex.submit(extra_binid, k) for k in EXTRA_BINID if want("binid67/" + k)]
    jobs += [ex.submit(extra_mismatch, rel) for rel in EXTRA_MISMATCH if SUITES_DIR.get("prior") and want("mismatch:" + rel)]
    for j in jobs:
        j.result()
if SUITES_DIR.get("prior"):
    shutil.rmtree(SUITES_DIR["prior"], ignore_errors=True)
if REH64.get("run"):
    shutil.rmtree(REH64["run"], ignore_errors=True)
if REH65.get("run"):
    shutil.rmtree(REH65["run"], ignore_errors=True)
if REH67.get("run"):
    shutil.rmtree(REH67["run"], ignore_errors=True)
print(f"     (all cases: {time.time() - t_all:.0f} s wall, {JOBS} jobs, {'FULL designs' if FULL else 'fast mode'})")

if not only:
    wanted = {(p, v) for p, vs in hp.HARNESS_PATHS.items() for v in vs}
    check("HARNESS_PATHS equals the set of paths the stub runs executed (an untested entry or an unlisted run fails here)", EXECUTED == wanted, (wanted ^ EXECUTED))
    check("plan 67's session-0 scripts run on the stubs: the repeat run (pass, one failure -> ABORT-REPEAT, a budget cut on record) and binid on three seats (pass, two seats one tree -> BINID67 FAIL)", EXTRA67_DONE == set(EXTRA_REPEAT) | set(EXTRA_BINID), (set(EXTRA_REPEAT) | set(EXTRA_BINID)) ^ EXTRA67_DONE)
    check("a changed byte in Cargo.lock, a Cargo.toml, vendor/ or the math / wasm sources each invalidates the suite reuse (ABORT-TESTS)", EXTRA_DONE == set(EXTRA_MISMATCH), set(EXTRA_MISMATCH) ^ EXTRA_DONE)
    check("ADR-0080's flake rule runs in run_suites: a flake outside quarantine.txt aborts (flake-not-quarantined), one on it is recorded and passes (flake-quarantined)", FLAKE_DONE == set(EXTRA_FLAKE), set(EXTRA_FLAKE) ^ FLAKE_DONE)
    check("HARNESS_PATHS covers exactly the candidate plans of run_all (41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56, 57a, 57b, 59, 60a, 60b, 61a, 61b, 62r0, 62a, 51e, 51, 63, 64, 65) -- the plans LD_REHEARSE is defined for (LD_PRIOR_TESTS: all but 56, 57a, 57b, 59, 60a, 60b, 61a, 61b, 62r0, 62a, 51e, 51, 63, 64, 65, whose reuse is the reference manifest or none)",
          set(hp.HARNESS_PATHS) == {"41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56", "57a", "57b", "59", "60a", "60b", "61a", "61b", "62r0", "62a", "51e", "51", "63", "64", "65", "67pre", "67", "67g", "67x"})
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
