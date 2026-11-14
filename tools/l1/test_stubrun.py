#!/usr/bin/env python3
"""Stub runs of the candidate plans' launch paths (plans 41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56): loopdiag_run_all.sh EXECUTED end to end, against stubs.

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
    case "$PWD" in */base/src/raptorpath) kind=BASE ;; *) kind=CAND ;; esac
    { echo '#!/bin/sh'; echo "# stub engine binary $kind built from $PWD"; echo "# RWM_STORE"
      [ "$kind" = CAND ] && echo "# [QCC] t= RWM_QUIC_CC"
      echo "# RWM_STORE_PATH_POOL"                                          # plan 54b: the knob is in both trees
      echo "# RWM_SUCC_DUMP"                                                # plan 56: the raw successor dump is in main
      { [ "$kind" = CAND ] || [ "$LD_PLAN" = 54b ]; } && echo "# [SNAPV] site= tail"       # plan 54: only CAND (main + the law) has the gauge; plan 54b: both trees (cand54, levb54)
      echo "# fmt  RWM_PLACE_WDIV= RWM_RECOV_SNAPVER= tail"       # plan 53: both trees carry the WDIV and SNAPVER echoes (CW0 runs on CAND's, BASES on BASE's)
      echo 'exit 0'; } > "$T/release/raptorpath"
    chmod +x "$T/release/raptorpath" ;;
  test) echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" ;;
esac
exit 0
''',
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
python3 ./stubrun_gen.py "$@" || { echo "stub driver failed"; exit 1; }
exit 0
'''
CAND_SHA = "c" * 40
BASE_SHA = "b3000701f440d11b1b5ad946c8b36c3623c770cd"
HIST_BASE = "4926822" + "0" * 33
# FULL (STUBRUN_FULL=1): the registered designs, 96 guard rows / 22 path-event rows plus the smokes. FAST (the default): the sandbox's COPY of the tables and of
# the design-size checks is shrunk (2 groups = 16 guard rows, 8 path-event rows), so every path and marker is exercised in minutes; the same code runs, on fewer rows.
FULL = os.environ.get("STUBRUN_FULL") == "1"
# plan 52: the guard's smoke (2) + the guard + the reported block's smoke (3) + the block (30 registered, 10 in fast mode)
EXPECT_ROWS = ({("41c", False): 2, ("41c", True): 98, ("46", False): 2, ("46", True): 24, ("47", False): 4, ("47", True): 122, ("48", False): 2, ("48", True): 98,
                ("52", False): 5, ("52", True): 131, ("41d", False): 3, ("41d", True): 99, ("53", False): 8, ("53", True): 68, ("54", False): 3, ("54", True): 99, ("54b", False): 6, ("54b", True): 42, ("56", False): 4, ("56", True): 68} if FULL else
               {("41c", False): 2, ("41c", True): 18, ("46", False): 2, ("46", True): 10, ("47", False): 4, ("47", True): 28, ("48", False): 2, ("48", True): 18,
                ("52", False): 5, ("52", True): 31, ("41d", False): 3, ("41d", True): 19, ("53", False): 8, ("53", True): 24, ("54", False): 3, ("54", True): 19, ("54b", False): 6, ("54b", True): 18, ("56", False): 4, ("56", True): 22})
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
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 64 ] || { echo "ABORT-PLAN56 table', '[ "${#tbl[@]}" -eq 18 ] || { echo "ABORT-PLAN56 table', 1),
    ("place38_parse.py", "REPS_CAP56 = None  ", "REPS_CAP56 = 1  ", 1),
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
    with open(os.path.join(tools, "perf_rwm_c.sh"), "w", newline="\n") as fh:
        fh.write(PERF_STUB)
    if not FULL:
        shrink(tools)
    tree = {"raptorpath/src/lib.rs": src_text, "raptorpath/src/b.rs": "// b\n", "raptorpath/tests/t.rs": "// t\n",
            "Cargo.toml": "[workspace]\n", "Cargo.lock": "# lock\n", "raptorpath-math/src/lib.rs": "// math\n", "raptorpath-wasm/src/lib.rs": "// wasm\n",
            "gf256/src/lib.rs": "// gf",
            "vendor/quinn-proto/Cargo.toml": "[package]\n", "vendor/quinn-proto/src/lib.rs": "// vendored\n"}
    for crate in ("gf256", "raptorpath", "raptorpath-math", "raptorpath-wasm"):
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
    for n in ("rows.log", "cargo.log", "drv.log"):
        open(os.path.join(sb["logs"], n), "w").close()
    env = dict(os.environ)
    for k in [k for k in env if k.startswith(("LD_", "RWM_", "STUB_"))]:
        del env[k]
    env.update({
        "PATH": px(sb["bin"]) + os.pathsep + env.get("PATH", ""),
        "LD_ROOT": px(sb["root"]), "LD_SHM": px(sb["devshm"]) + "/ld", "LD_DEVSHM": px(sb["devshm"]), "LD_PROCFS": px(sb["proc"]), "LD_TMPD": px(sb["tmpd"]),
        "LD_LOCK_VM": px(sb["locks"]) + "/vm.lock", "LD_LOCK_RP": px(sb["locks"]) + "/rp.lock",
        "LD_PLAN": plan, "LD_CAND_SHA": CAND_SHA, "LD_HARD": str(int(time.time()) + 4 * 3600),
        "STUB_ROWLOG": px(sb["logs"]) + "/rows.log", "STUB_CARGO_LOG": px(sb["logs"]) + "/cargo.log", "STUB_DRVLOG": px(sb["logs"]) + "/drv.log",
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


MANIFEST_DIRS = ("raptorpath/src", "raptorpath/tests", "raptorpath-math/src", "raptorpath-wasm/src", "gf256/src", "vendor")


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
EXTRA_MISMATCH = ("Cargo.lock", "Cargo.toml", "raptorpath-math/Cargo.toml", "vendor/quinn-proto/src/lib.rs", "raptorpath-wasm/src/lib.rs", "raptorpath-math/src/lib.rs", "gf256/src/lib.rs")


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
              rc == 0 and "LD-ALL-DONE" in out and len(d) == 1 and d[0]["status"] == "LIVE" and d[0]["dnf"] is True and d[0].get("mbps") is None, (rc, tail(out, 6), [(r["arm"], r["status"], r["dnf"]) for r in led][:4]))
        check("54b/dnf: the DNF row keeps old holes, inorder_mean_late, the budget residual and q0 computed from its [DIAG] / [RANK] lines (none None)",
              all(t.get(k) is not None for k in ("old_holes", "inorder_mean_late", "inorder_mean_run", "resid_late", "resid_rel", "q0_late", "rtt0_late")), t)
        check("54b/dnf: the report counts it (dnf=1 in the verdict line, 'DNF 1' for B640) and the falsifiers still evaluate",
              "dnf=1" in sc and re.search(r"B640\s+rows \d+.*DNF 1", sc) is not None and "F1 " in sc, sc[-700:])
    finally:
        shutil.rmtree(root, ignore_errors=True)


def red54sha(plan, stage, want):
    """The sha witness failing once in a stub run: the sandbox's battery hands the row parser a wrong sha, the rehearsal's smoke must FAIL on the bin_sha criterion."""
    root = tempfile.mkdtemp(prefix="stubrun-")
    try:
        sb = make_sandbox(root, plan, None if plan == "56" else BASE_SHA)
        bt = os.path.join(sb["repo"], "tools", "l1", "loopdiag_battery.sh")
        txt = open(bt, newline="").read()
        old = '"$TMPD/rwm-s.log" "$cot" "$sha" >> "$OUT" 2>&1'
        assert txt.count(old) == 1, txt.count(old)
        with open(bt, "w", newline="") as fh:
            fh.write(txt.replace(old, '"$TMPD/rwm-s.log" "$cot" "0000deadbeef" >> "$OUT" 2>&1'))
        rc, out, rows, cargo = run_launch(sb, plan, None if plan == "56" else BASE_SHA, rehearse=True)
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
              rc == 0 and "LD-ALL-DONE" in out and len(led) == (66 if FULL else 20) and len(per) == (64 if FULL else 18) and max(len(v) for v in per.values()) == 2, (rc, len(led), len(per), tail(out, 6)))
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


def refused56(name, extra, want_rc, want_in_era, no_build=True):
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
              rc == want_rc and want_in_era in e + out and os.path.exists(os.path.join(sb["root"], "run", "FAILED-ALL")) and rows == [] and (not no_build or not [c for c in cargo if "|build " in c]), (rc, tail(e, 6), tail(out, 6)))
    finally:
        shutil.rmtree(root, ignore_errors=True)


SUITES_DIR = {}          # plan -> persisted dir of the plan's full run (the green suite run the prior-tests cases reuse)


def case(plan, path):
    base_sha = None if (path == "historical-base" or plan == "56") else BASE_SHA
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
        ref_env = {"LD_REF_MANIFEST": ref56_file(sb, miss=(path == "reuse-miss"))} if (plan == "56" and path in ("reuse-hit", "reuse-miss")) else None
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
        if plan == "56":
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
            sdir = os.path.join(run_dir, "suites") if plan in ("47", "48", "52", "41d", "53", "54", "54b", "56") else run_dir
            recorded = [f for f in os.listdir(sdir) if f in ("TESTS-OK", "SRC-MANIFEST.sha256")] if os.path.isdir(sdir) else []
            if plan == "46":
                check(f"{tag}: plan 46 without LD_P46_SUITES runs no suites (no cargo test, no TESTS-OK)", tests == [] and recorded == [], (len(tests), recorded))
            elif path == "reuse-hit":
                tt = open(os.path.join(sdir, "TESTS.txt"), errors="replace").read()
                check(f"{tag}: the reference manifest HIT skips the engine suites (no cargo test, 'ENGINE-SUITES-REUSED' in TESTS.txt) but runs the tools tests; it leaves TOOLS-TESTS-OK and NO TESTS-OK / SRC-MANIFEST.sha256 (a false suite record another plan's LD_PRIOR_TESTS would accept)",
                      tests == [] and "ENGINE-SUITES-REUSED" in tt and "TEST parsers rc=0" in tt and "TEST bashtests rc=0" in tt and "TEST main" not in tt and not os.path.exists(os.path.join(sdir, "TESTS-OK")) and not os.path.exists(os.path.join(sdir, "SRC-MANIFEST.sha256"))
                      and os.path.exists(os.path.join(sdir, "TOOLS-TESTS-OK")), (len(tests), tt, os.listdir(sdir)))
            else:
                check(f"{tag}: the suites ran (cargo test x4{', the reference manifest MISSED' if plan == '56' else ''}) and TESTS-OK + SRC-MANIFEST.sha256 were recorded at suite time",
                      len(tests) == 4 and sorted(recorded) == ["SRC-MANIFEST.sha256", "TESTS-OK"], (len(tests), recorded))
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


all_cases = [(p, v) for p in ("41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56") for v in hp.HARNESS_PATHS[p]]
phase1 = [(p, v) for p, v in all_cases if not v.startswith("prior-tests")]
phase2 = [(p, v) for p, v in all_cases if v.startswith("prior-tests")]
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
    if want("prior56"):
        jobs.append(ex.submit(refused56, "prior", {"LD_PRIOR_TESTS": "x"}, 5, "ABORT-TESTS plan 56 reuses the engine suites by the reference manifest", False))
    for j in jobs:
        j.result()
    jobs = [ex.submit(timed, p, v) for p, v in phase2 if want(f"{p}/{v}")]
    jobs += [ex.submit(extra_mismatch, rel) for rel in EXTRA_MISMATCH if SUITES_DIR.get("prior") and want("mismatch:" + rel)]
    for j in jobs:
        j.result()
if SUITES_DIR.get("prior"):
    shutil.rmtree(SUITES_DIR["prior"], ignore_errors=True)
print(f"     (all cases: {time.time() - t_all:.0f} s wall, {JOBS} jobs, {'FULL designs' if FULL else 'fast mode'})")

if not only:
    wanted = {(p, v) for p, vs in hp.HARNESS_PATHS.items() for v in vs}
    check("HARNESS_PATHS equals the set of paths the stub runs executed (an untested entry or an unlisted run fails here)", EXECUTED == wanted, (wanted ^ EXECUTED))
    check("a changed byte in Cargo.lock, a Cargo.toml, vendor/ or the math / wasm sources each invalidates the suite reuse (ABORT-TESTS)", EXTRA_DONE == set(EXTRA_MISMATCH), set(EXTRA_MISMATCH) ^ EXTRA_DONE)
    check("HARNESS_PATHS covers exactly the candidate plans of run_all (41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56) -- the plans LD_REHEARSE is defined for (LD_PRIOR_TESTS: all but 56, whose reuse is the reference manifest)",
          set(hp.HARNESS_PATHS) == {"41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56"})
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
