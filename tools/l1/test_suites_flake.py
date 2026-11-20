#!/usr/bin/env python3
"""suites_run.sh tst() under ADR-0080: a failure that passes the solo re-run is FLAKE-FAILURE and
fails the suite (rc 1); with the test on raptorpath/tests/quarantine.txt it is QUARANTINED (rc 0).
tst() is extracted from the real script and run under bash against a stub cargo."""
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
bash = shutil.which("bash")
fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name, "" if ok else detail)
    fails += not ok


src = open(os.path.join(HERE, "suites_run.sh"), encoding="utf-8").read()
m = re.search(r'^QUAR=.*?^tst\(\) \{.*?^\}\n', src, re.S | re.M)
check("tst() is extractable from suites_run.sh and bash exists", bool(m and bash))


def run(quarantine):
    td = tempfile.mkdtemp()
    try:
        s = os.path.join(td, "src")
        os.makedirs(os.path.join(s, "tools/ci"))
        os.makedirs(os.path.join(s, "raptorpath/tests"))
        shutil.copy(os.path.join(REPO, "tools/ci/flake_rule.py"), os.path.join(s, "tools/ci/flake_rule.py"))
        with open(os.path.join(s, "raptorpath/tests/quarantine.txt"), "w", newline="\n") as f:
            f.write(quarantine)
        bind = os.path.join(td, "bin")
        os.makedirs(bind)
        # first run: one test FAILED (rc 101); the solo re-run (`--exact`) passes
        stub = ('#!/bin/bash\nif [[ "$*" == *--exact* ]]; then echo "test net::flaky_one ... ok"; exit 0; fi\n'
                'echo "test net::flaky_one ... FAILED"; echo "test result: FAILED. 0 passed; 1 failed; 0 ignored"; exit 101\n')
        with open(os.path.join(bind, "cargo"), "w", newline="\n") as f:
            f.write(stub)
        os.chmod(os.path.join(bind, "cargo"), 0o755)
        root = os.path.join(td, "root")
        os.makedirs(root)
        p = lambda x: x.replace("\\", "/")
        script = (f"SRC='{p(s)}'\nROOT='{p(root)}'\nOUT=\"$ROOT/TESTS.txt\"\nera() {{ :; }}\niostate() {{ echo x; }}\n: > \"$OUT\"\n"
                  + m.group(0) + f"tst t '{p(os.path.join(bind, 'cargo'))}' test -p x -- --test-threads=2; echo rc=$?\n")
        cp = subprocess.run([bash, "-c", script], capture_output=True, text=True, timeout=120,
                            env=dict(os.environ, PATH=p(bind) + os.pathsep + os.environ.get("PATH", "")))
        return cp.stdout + cp.stderr, open(os.path.join(root, "TESTS.txt"), errors="replace").read()
    finally:
        shutil.rmtree(td, ignore_errors=True)


if m and bash:
    out, tt = run("# empty\n")
    check("red: a failure that passes its solo re-run, test not quarantined: rc=1 and FLAKE-FAILURE", "rc=1" in out and "FLAKE-FAILURE t net::flaky_one" in tt, (out, tt))
    out, tt = run("flaky_one | alice | timing bound, status 12\n")
    check("green: the same failure with the test on the quarantine list: rc=0 and QUARANTINED", "rc=0" in out and "QUARANTINED t net::flaky_one" in tt and "FLAKE-FAILURE t" not in tt, (out, tt))
sys.exit(fails)
