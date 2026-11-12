#!/usr/bin/env python3
"""harness_paths.HARNESS_PATHS: the table of launch paths per candidate plan. Its shape and its tie to the scripts are asserted here; that every entry
is EXECUTED is asserted by test_stubrun.py (it runs each entry and compares the executed set with this table)."""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import harness_paths as hp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
KINDS = {"rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base"}
T = hp.HARNESS_PATHS

check("the table covers the candidate plans 41c, 46, 47, 48, 52 and no other", set(T) == {"41c", "46", "47", "48", "52"}, sorted(T))
check("every path name is a known kind and none repeats", all(set(v) <= KINDS and len(set(v)) == len(v) for v in T.values()), T)
check("every plan has the rehearse, full, prior-tests and prior-tests-mismatch paths", all({"rehearse", "full", "prior-tests", "prior-tests-mismatch"} <= set(v) for v in T.values()))
check("historical-base (LD_BASE_SHA unset) is listed for every plan that allows it, and not for plans 48 and 52, which require LD_BASE_SHA",
      all(("historical-base" in v) == (p not in ("48", "52")) for p, v in T.items()) and ': "${LD_BASE_SHA:?' in runall)
m = re.search(r'case "\$PLAN" in 41c\|46\|47\|48\|52\) ;; \*\) echo "ABORT-REHEARSE', runall)
check("loopdiag_run_all.sh defines LD_REHEARSE for exactly the plans of the table (other plans: ABORT-REHEARSE)", m is not None)
check("every plan of the table is selectable in run_all's plan case", all(re.search(r"^  %s\) P_SMOKE=" % re.escape(p), runall, re.M) for p in T))
check("LD_PRIOR_TESTS is wired into the suites step of the table's plans (suites_stage) and records its manifest at suite time",
      "suites_stage() {" in runall and "reuse_prior_tests() {" in runall and 'src_manifest > "$SD/SRC-MANIFEST.sha256"' in runall)

# test_stubrun.py must never run inside a session's `tst parsers` suite (it re-runs the harness against stubs). Run the script's own command for real.
import shutil
import subprocess
import tempfile
m = re.search(r"tst parsers bash -c '([^']*)'", runall)
check("run_suites' parsers command is found", m is not None)
bash = shutil.which("bash")
if m and bash:
    td = tempfile.mkdtemp()
    try:
        d = os.path.join(td, "raptorpath", "tools", "l1")
        os.makedirs(d)
        for name in ("test_a.py", "test_stubrun.py", "test_z.py"):
            with open(os.path.join(d, name), "w") as fh:
                fh.write("open('ran_%s', 'w').close()\n" % name[5:-3])
        shim = os.path.join(td, "bin")
        os.makedirs(shim)
        with open(os.path.join(shim, "python3"), "w", newline="\n") as fh:      # the command says python3; run it with this interpreter
            fh.write('#!/bin/sh\nexec "%s" "$@"\n' % sys.executable.replace("\\", "/"))
        os.chmod(os.path.join(shim, "python3"), 0o755)
        env = dict(os.environ, PATH=shim.replace("\\", "/") + os.pathsep + os.environ.get("PATH", ""))
        cp = subprocess.run([bash, "-c", m.group(1)], cwd=td, env=env, capture_output=True, text=True)
        ran = sorted(f for f in os.listdir(d) if f.startswith("ran_"))
        check("the parsers suite runs every test_*.py except test_stubrun.py (explicit skip)", cp.returncode == 0 and ran == ["ran_a", "ran_z"], (cp.returncode, ran, cp.stderr[-300:]))
    finally:
        shutil.rmtree(td, ignore_errors=True)
elif m:
    print("SKIP no bash for the parsers-command check")
check("test_stubrun.py is documented as harness-changes-only in run_all and in its own docstring",
      "belongs to harness changes only" in runall and "harness changes only" in open(os.path.join(HERE, "test_stubrun.py"), encoding="utf-8").read())
man = re.search(r"src_manifest\(\) \{.*?\n\}", runall, re.S)
check("the suite-reuse manifest covers src, tests, math / wasm src, vendor/, the workspace Cargo.toml / Cargo.lock and every crate's Cargo.toml (a dependency patch invalidates a reuse)",
      man is not None and all(x in man.group(0) for x in ("raptorpath/src", "raptorpath/tests", "raptorpath-math/src", "raptorpath-wasm/src", "gf256/src", "vendor", "Cargo.toml", "Cargo.lock", "./*/Cargo.toml")), man and man.group(0))
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
