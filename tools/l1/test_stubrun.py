#!/usr/bin/env python3
"""Stub runs of the candidate plans' launch paths (plans 41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56, 57a, 57b, 59, 60a, 60b, 64): loopdiag_run_all.sh EXECUTED end to end, against stubs.

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
case "$1" in
  build)
    T="${CARGO_TARGET_DIR:?}"; mkdir -p "$T/release"
    if [ -n "${STUB_BIN_FILE:-}" ]; then cp "$STUB_BIN_FILE" "$T/release/raptorpath"; chmod +x "$T/release/raptorpath"; exit 0; fi      # plans 60a / 60b: the test knows the sha it registers
    case "$PWD" in */base/src/raptorpath) kind=BASE ;; *) kind=CAND ;; esac
    { echo '#!/bin/sh'; echo "# stub engine binary $kind built from $PWD"; echo "# RWM_STORE"
      [ "$kind" = CAND ] && echo "# [QCC] t= RWM_QUIC_CC"
      echo "# RWM_STORE_PATH_POOL"                                          # plan 54b: the knob is in both trees
      echo "# RWM_SUCC_DUMP"                                                # plan 56: the raw successor dump is in main
      echo "# RWM_EMIT_BURST"                                               # plans 57a / 57b: the emission-burst knob is in main
      echo "# RWM_UNIFIED_SHED unified overload shedding ACTIVE"            # plan 59: the shed gate and its liveness echo are in main
      echo "# RWM_SLAW [SLAW] t="                                           # plans 60a / 60b: the gauge
      { [ "$kind" = CAND ] || [ "$LD_PLAN" = 54b ]; } && echo "# [SNAPV] site= tail"       # plan 54: only CAND (main + the law) has the gauge; plan 54b: both trees (cand54, levb54)
      echo "# fmt  RWM_PLACE_WDIV= RWM_RECOV_SNAPVER= tail"       # plan 53: both trees carry the WDIV and SNAPVER echoes (CW0 runs on CAND's, BASES on BASE's)
      { [ "$kind" = CAND ] && [ "${LD_PLAN:-}" = 64 ]; } && echo "# [SYMSZ] [UNFRAMABLE] F2 framing"      # plan 64: the F2 build holds both strings, main neither
      { [ "$kind" = BASE ] && [ -n "${STUB_BASE_SYMSZ:-}" ]; } && echo "# [SYMSZ] main with a stray size echo"      # plan 64's base-has-symsz path
      echo 'exit 0'; } > "$T/release/raptorpath"
    chmod +x "$T/release/raptorpath" ;;
  test)
    case " $* " in
      *" --no-run "*) ;;
      *" framing_ns "*)      # plan 64's rehearsal: one run of the framing bench per call, the STUB_BENCH_DIR blocks b1..b3 in rotation
        n=$(cat "$STUB_BENCH_DIR/n" 2>/dev/null || echo 0); n=$(( n + 1 )); echo "$n" > "$STUB_BENCH_DIR/n"
        cat "$STUB_BENCH_DIR/b$(( (n - 1) % 3 + 1 )).txt" ;;
      *) echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" ;;
    esac ;;
esac
exit 0
''',
    "taskset": '#!/bin/bash\n[ "$1" = "-c" ] && { echo "taskset $*" >> "${STUB_TASKSET_LOG:-/dev/null}"; shift 2; }\nexec "$@"\n',
    "python3": r'''#!/bin/bash
# python3 passthrough that records the environment each ROW parser invocation sees (loopdiag_battery.sh: python3 <parser> row <cell> <arm> ...)
if [ "$2" = "row" ]; then echo "plan=${LD_PLAN-<unset>} base_sha=${LD_BASE_SHA-<unset>} cell=$3 arm=$4 parser=$1" >> "$STUB_ROWLOG"; fi
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
[ -n "${STUB_DRVLOG2:-}" ] && echo "$3 reliable=${RWM_C_RELIABLE-unset} unified=${RWM_UNIFIED-unset} gen=${RWM_GEN-unset}" >> "$STUB_DRVLOG2"      # plan 59: hint + the evicting-seat env (a separate log: drv.log's lines are compared whole)
[ -n "${STUB_DRVLOG5:-}" ] && echo "$3 $4 $6 timeout=${RWM_PERF_TIMEOUT_S-unset} reliable=${RWM_C_RELIABLE-unset} diag=${RWM_DIAG-unset} bindir=$(basename "$(dirname "${RWM_BIN:-?/?}")")" >> "$STUB_DRVLOG5"      # plan 64: hint, bytes, mode, timeout, reliability, the seat's binary dir
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
SINGLE = ("56", "57a", "57b", "59", "60a", "60b")        # the single-seat plans: one build, no LD_BASE_SHA, suite reuse by the reference manifest
S60 = ("60a", "60b")
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
                fh.write("print('stub test (the real one runs in the suite)')\n" if f.endswith(".py") else "exit 0\n")
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
    if not FULL:
        shrink(tools)
    tree = {"raptorpath/src/lib.rs": src_text, "raptorpath/src/b.rs": "// b\n", "raptorpath/tests/t.rs": "// t\n",
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
    for n in ("rows.log", "cargo.log", "drv.log", "drv2.log", "drv3.log", "drv5.log"):
        open(os.path.join(sb["logs"], n), "w").close()
    env = dict(os.environ)
    for k in [k for k in env if k.startswith(("LD_", "RWM_", "STUB_"))]:
        del env[k]
    env.update({
        "PATH": px(sb["bin"]) + os.pathsep + env.get("PATH", ""),
        "LD_ROOT": px(sb["root"]), "LD_SHM": px(sb["devshm"]) + "/ld", "LD_DEVSHM": px(sb["devshm"]), "LD_PROCFS": px(sb["proc"]), "LD_TMPD": px(sb["tmpd"]),
        "LD_LOCK_VM": px(sb["locks"]) + "/vm.lock", "LD_LOCK_RP": px(sb["locks"]) + "/rp.lock",
        "LD_PLAN": plan, "LD_CAND_SHA": CAND_SHA, "LD_HARD": str(int(time.time()) + 6 * 3600),
        "STUB_ROWLOG": px(sb["logs"]) + "/rows.log", "STUB_CARGO_LOG": px(sb["logs"]) + "/cargo.log", "STUB_DRVLOG": px(sb["logs"]) + "/drv.log", "STUB_DRVLOG2": px(sb["logs"]) + "/drv2.log", "STUB_DRVLOG3": px(sb["logs"]) + "/drv3.log", "STUB_DRVLOG5": px(sb["logs"]) + "/drv5.log", "STUB_TASKSET_LOG": px(sb["logs"]) + "/taskset.log",
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
    check(f"{tag}: the readouts are the stub's known deltas (per tick 250 src / 240 acked / 20 ack wakes, n = 100 + k % 5): release batch 12.5 (src) / 12.0 (ack), the shed counted from [SPAN], the seat armed",
          abs(t["rel_src"] - 12.5) < 1e-9 and abs(t["rel_ack"] - 12.0) < 1e-9 and t["mbar_lo"] is not None and t["w1_hi"] is not None and t["shed_total"] > 0 and t["shed_armed_span"] and t["sender_shed_echo"] and t["receiver_shed_echo"], t)
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
    body = ("#!/bin/sh\n# stub engine binary for plans 60a / 60b\n# RWM_STORE\n# RWM_STORE_PATH_POOL\n# RWM_UNIFIED_SHED unified overload shedding ACTIVE\n# RWM_SLAW\n# [SLAW] t=\n"
            "# RWM_SUCC_DUMP RWM_EMIT_BURST\nexit 0\n")
    with open(path, "w", newline="\n") as fh:
        fh.write(body)
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
        table = plan64.plan(plan_id="65")
        n_pairs = plan64.PAIRS65
        n_slots = len(table)
        extra_rows = {"base-dead-redrawn": 1, "base-dead-twice": 2, "unresolved-n": 4, "rho-one-end": 1}.get(path, 0)
        check(f"{tag}: the ledger holds the table's {n_slots} slots in rt65/ (+ {extra_rows} redraw row(s) the path asks for), every row with its t64 readings and its early-loss field; the stage ran to the end (DONE, DONE-ALL)",
              len(led) == n_slots + extra_rows and len({(r["arm"], r["seed"], r["rep"]) for r in led}) == n_slots and all(r.get("t64") and "qlost_2s" in r for r in led) and os.path.exists(os.path.join(stage, "DONE")) and os.path.exists(os.path.join(run_dir, "DONE-ALL")),
              (len(led), n_slots, extra_rows))
        ref = subprocess.run([sys.executable, os.path.join(sb["repo"], "tools", "l1", "s65_score.py"), os.path.join(stage, "ld.log").replace("\\", "/")], capture_output=True, text=True, errors="replace").stdout
        # the launch's scorer call passes the (shrunk) pair count and floor; the stand-alone file defaults to the registered 24 / 18, so compare through the same call
        buf = []
        sys.path.insert(0, os.path.join(sb["repo"], "tools", "l1"))
        s65_score.score([os.path.join(stage, "ld.log")], out=buf.append, n_pairs=n_pairs, min_pairs=plan64.MIN_PAIRS65)
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
            check(f"{tag}: the first two RB slots died on both draws: 'dead twice 2', two pairs dropped (not counted), {n_pairs - 2} usable pairs < the floor {plan64.MIN_PAIRS65}",
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


def case(plan, path):
    if plan in S60:
        return case60(plan, path)
    if plan == "65":
        return case65(plan, path)
    if plan == "64":
        return case64(plan, path)
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


all_cases = [(p, v) for p in ("41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56", "57a", "57b", "59", "60a", "60b", "64", "65") for v in hp.HARNESS_PATHS[p]]
phase1 = [(p, v) for p, v in all_cases if not v.startswith("prior-tests") and not (p == "64" and v in P64_AFTER) and not (p == "65" and v in P65_AFTER)]
phase2 = [(p, v) for p, v in all_cases if v.startswith("prior-tests") or (p == "64" and v in P64_AFTER) or (p == "65" and v in P65_AFTER)]      # plan 64's launch cases consume the rehearsal's binaries and tools result (REH64)
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
    for j in jobs:
        j.result()
    jobs = [ex.submit(timed, p, v) for p, v in phase2 if want(f"{p}/{v}")]
    jobs += [ex.submit(extra_mismatch, rel) for rel in EXTRA_MISMATCH if SUITES_DIR.get("prior") and want("mismatch:" + rel)]
    for j in jobs:
        j.result()
if SUITES_DIR.get("prior"):
    shutil.rmtree(SUITES_DIR["prior"], ignore_errors=True)
if REH64.get("run"):
    shutil.rmtree(REH64["run"], ignore_errors=True)
if REH65.get("run"):
    shutil.rmtree(REH65["run"], ignore_errors=True)
print(f"     (all cases: {time.time() - t_all:.0f} s wall, {JOBS} jobs, {'FULL designs' if FULL else 'fast mode'})")

if not only:
    wanted = {(p, v) for p, vs in hp.HARNESS_PATHS.items() for v in vs}
    check("HARNESS_PATHS equals the set of paths the stub runs executed (an untested entry or an unlisted run fails here)", EXECUTED == wanted, (wanted ^ EXECUTED))
    check("a changed byte in Cargo.lock, a Cargo.toml, vendor/ or the math / wasm sources each invalidates the suite reuse (ABORT-TESTS)", EXTRA_DONE == set(EXTRA_MISMATCH), set(EXTRA_MISMATCH) ^ EXTRA_DONE)
    check("HARNESS_PATHS covers exactly the candidate plans of run_all (41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56, 57a, 57b, 59, 60a, 60b, 64, 65) -- the plans LD_REHEARSE is defined for (LD_PRIOR_TESTS: all but 56, 57a, 57b, 59, 60a, 60b, 64, 65, whose reuse is the reference manifest)",
          set(hp.HARNESS_PATHS) == {"41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56", "57a", "57b", "59", "60a", "60b", "64", "65"})
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
