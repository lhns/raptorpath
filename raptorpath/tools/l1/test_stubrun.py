#!/usr/bin/env python3
"""Stub runs of the candidate plans' launch paths (plans 41c, 46, 47, 48): loopdiag_run_all.sh EXECUTED end to end, against stubs.

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
      [ "$kind" = CAND ] && echo "# [QCC] t= RWM_QUIC_CC"; echo 'exit 0'; } > "$T/release/raptorpath"
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
python3 ./stubrun_gen.py "$@" || { echo "stub driver failed"; exit 1; }
exit 0
'''
CAND_SHA = "c" * 40
BASE_SHA = "b3000701f440d11b1b5ad946c8b36c3623c770cd"
HIST_BASE = "4926822" + "0" * 33
# FULL (STUBRUN_FULL=1): the registered designs, 96 guard rows / 22 path-event rows plus the smokes. FAST (the default): the sandbox's COPY of the tables and of
# the design-size checks is shrunk (2 groups = 16 guard rows, 8 path-event rows), so every path and marker is exercised in minutes; the same code runs, on fewer rows.
FULL = os.environ.get("STUBRUN_FULL") == "1"
EXPECT_ROWS = ({("41c", False): 2, ("41c", True): 98, ("46", False): 2, ("46", True): 24, ("47", False): 4, ("47", True): 122, ("48", False): 2, ("48", True): 98} if FULL else
               {("41c", False): 2, ("41c", True): 18, ("46", False): 2, ("46", True): 10, ("47", False): 4, ("47", True): 28, ("48", False): 2, ("48", True): 18})
# (file, old, new, expected count): each must match, so a drift in the scripts fails here instead of silently running the full design
FAST_PATCHES = [
    ("loopdiag_run_all.sh", '[ "$nrows" != "96" ]', '[ "$nrows" != "16" ]', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 96 ] || { echo "ABORT-PLAN41C', '[ "${#tbl[@]}" -eq 16 ] || { echo "ABORT-PLAN41C', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 96 ] || { echo "ABORT-PLAN48', '[ "${#tbl[@]}" -eq 16 ] || { echo "ABORT-PLAN48', 1),
    ("loopdiag_battery.sh", '[ "${#tbl[@]}" -eq 22 ] || { echo "ABORT-PLAN46', '[ "${#tbl[@]}" -eq 8 ] || { echo "ABORT-PLAN46', 1),
    ("place38_parse.py", "EXPECT_ROWS41C = len(GUARD_CELLS) * (12 + 12)", "EXPECT_ROWS41C = len(GUARD_CELLS) * (2 + 2)", 1),
    ("place38_parse.py", "for rp in range(1, 7) for sd", "for rp in range(1, 2) for sd", 1),
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
    tools = os.path.join(sb["repo"], "raptorpath", "tools", "l1")
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


def run_launch(sb, plan, base_sha, rehearse=False, prior=None, p46_suites=False, timeout=3600):
    """Run loopdiag_run_all.sh in the sandbox; -> (rc, stdout, rowlog lines, cargo log lines)."""
    for n in ("rows.log", "cargo.log"):
        open(os.path.join(sb["logs"], n), "w").close()
    env = dict(os.environ)
    for k in [k for k in env if k.startswith(("LD_", "RWM_", "STUB_"))]:
        del env[k]
    env.update({
        "PATH": px(sb["bin"]) + os.pathsep + env.get("PATH", ""),
        "LD_ROOT": px(sb["root"]), "LD_SHM": px(sb["devshm"]) + "/ld", "LD_DEVSHM": px(sb["devshm"]), "LD_PROCFS": px(sb["proc"]), "LD_TMPD": px(sb["tmpd"]),
        "LD_LOCK_VM": px(sb["locks"]) + "/vm.lock", "LD_LOCK_RP": px(sb["locks"]) + "/rp.lock",
        "LD_PLAN": plan, "LD_CAND_SHA": CAND_SHA, "LD_HARD": str(int(time.time()) + 4 * 3600),
        "STUB_ROWLOG": px(sb["logs"]) + "/rows.log", "STUB_CARGO_LOG": px(sb["logs"]) + "/cargo.log",
        "STUB_PYTHON": px(sys.executable), "STUB_PYSITE": os.path.abspath(sb["bin"]), "STUB_RAW": os.environ.get("STUBRUN_RAW") or os.path.abspath(os.path.join(HERE, "..", "..", "docs", "l1-raw")),
    })
    if base_sha:
        env["LD_BASE_SHA"] = base_sha
    if rehearse:
        env["LD_REHEARSE"] = "1"
    if prior:
        env["LD_PRIOR_TESTS"] = px(prior)
    if p46_suites:
        env["LD_P46_SUITES"] = "1"
    script = px(os.path.join(sb["repo"], "raptorpath", "tools", "l1", "loopdiag_run_all.sh"))
    cp = subprocess.run([BASH, script], env=env, capture_output=True, text=True, errors="replace", timeout=timeout)
    rows = open(os.path.join(sb["logs"], "rows.log")).read().splitlines()
    cargo = open(os.path.join(sb["logs"], "cargo.log")).read().splitlines()
    return cp.returncode, cp.stdout + cp.stderr, rows, cargo


def era(sb):
    p = os.path.join(sb["root"], "run", "all-era.txt")
    return open(p, errors="replace").read() if os.path.exists(p) else ""


MANIFEST_DIRS = ("raptorpath/src", "raptorpath/tests", "raptorpath-math/src", "raptorpath-wasm/src", "vendor")


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
EXTRA_MISMATCH = ("Cargo.lock", "Cargo.toml", "raptorpath-math/Cargo.toml", "vendor/quinn-proto/src/lib.rs", "raptorpath-wasm/src/lib.rs", "raptorpath-math/src/lib.rs")


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

SUITES_DIR = {}          # plan -> persisted dir of the plan's full run (the green suite run the prior-tests cases reuse)


def case(plan, path):
    base_sha = None if path == "historical-base" else BASE_SHA
    rehearse = path != "full"
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
        rc, out, rows, cargo = run_launch(sb, plan, base_sha, rehearse=rehearse, prior=prior, p46_suites=(plan == "41c"))
        e = era(sb)
        failed = os.path.exists(os.path.join(sb["root"], "run", "FAILED-ALL"))
        if path == "prior-tests-mismatch":
            check(f"{tag}: one changed byte in raptorpath/src -> ABORT-TESTS, FAILED-ALL, rc 5, no smoke row measured",
                  rc == 5 and "ABORT-TESTS" in e and failed and rows == [] and "LD-ALL-REHEARSAL-DONE" not in out, (rc, tail(out)))
            EXECUTED.add((plan, path))
            return
        marker = "LD-ALL-REHEARSAL-DONE" if rehearse else "LD-ALL-DONE"
        n = EXPECT_ROWS[(plan, not rehearse)]
        ok_marker = rc == 0 and marker in out and not failed
        check(f"{tag}: rc 0 and the terminal marker {marker}", ok_marker, (rc, tail(out, 40), tail(e, 15)))
        bases = {re.search(r"base_sha=(\S*)", r).group(1) for r in rows}
        want = base_sha or ""
        check(f"{tag}: {n} row-parser calls, each with LD_BASE_SHA={want or '(empty)'} having passed `sudo -n env` and each under LD_PLAN of its stage",
              len(rows) == n and bases == {want}, (len(rows), bases))
        plans = {r.split("plan=")[1].split()[0] for r in rows}
        check(f"{tag}: the row parser ran under the plan(s) {'41c+46' if plan == '47' else plan}", plans == ({"41c", "46"} if plan == "47" else {plan}), plans)
        tests = [c for c in cargo if "|test " in c]
        builds = [c for c in cargo if "|build " in c]
        check(f"{tag}: two builds (CAND tree, BASE tree)", len(builds) == 2 and sum("/base/src/" in b for b in builds) == 1, builds)
        run_dir = os.path.join(sb["root"], "run")
        if rehearse and path == "rehearse":
            check(f"{tag}: rehearsal ran no suites and no battery (no cargo test, no ld.log, REHEARSAL-PASS persisted)",
                  tests == [] and not any(f == "ld.log" for _, _, fs in os.walk(run_dir) for f in fs)
                  and any(f == "REHEARSAL-PASS" for _, _, fs in os.walk(run_dir) for f in fs), (tests, list(os.walk(run_dir))[:3]))
        if path == "historical-base":
            check(f"{tag}: BASE pinned to 4926822 (no LD_BASE_SHA anywhere): no cargo test, REHEARSAL-PASS", tests == [] and "REHEARSAL-PASS" in e)
        if path == "prior-tests":
            check(f"{tag}: the green suite run of the identical tree is reused: cargo test never runs, TESTS-OK and the manifest are carried over",
                  tests == [] and "reused TESTS-OK" in e and os.path.exists(os.path.join(run_dir, "suites" if plan in ("47", "48") else "", "TESTS-OK")), (tests, tail(e, 12)))
        if path == "full":
            sdir = os.path.join(run_dir, "suites") if plan in ("47", "48") else run_dir
            recorded = [f for f in os.listdir(sdir) if f in ("TESTS-OK", "SRC-MANIFEST.sha256")] if os.path.isdir(sdir) else []
            if plan == "46":
                check(f"{tag}: plan 46 without LD_P46_SUITES runs no suites (no cargo test, no TESTS-OK)", tests == [] and recorded == [], (len(tests), recorded))
            else:
                check(f"{tag}: the suites ran (cargo test x4) and TESTS-OK + SRC-MANIFEST.sha256 were recorded at suite time",
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
        ra = os.path.join(sb["repo"], "raptorpath", "tools", "l1", "loopdiag_run_all.sh")
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


all_cases = [(p, v) for p in ("41c", "46", "47", "48") for v in hp.HARNESS_PATHS[p]]
phase1 = [(p, v) for p, v in all_cases if not v.startswith("prior-tests")]
phase2 = [(p, v) for p, v in all_cases if v.startswith("prior-tests")]
t_all = time.time()
with ThreadPoolExecutor(max_workers=JOBS) as ex:
    # the slow ones first (a full run is minutes; a rehearsal under a minute)
    jobs = [ex.submit(timed, p, v) for p, v in sorted(phase1, key=lambda c: c[1] != "full") if want(f"{p}/{v}")]
    if want("mutation"):
        jobs.append(ex.submit(mutation))
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
    check("HARNESS_PATHS covers exactly the candidate plans of run_all (41c, 46, 47, 48) -- the plans LD_REHEARSE and LD_PRIOR_TESTS are defined for",
          set(hp.HARNESS_PATHS) == {"41c", "46", "47", "48"})
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
