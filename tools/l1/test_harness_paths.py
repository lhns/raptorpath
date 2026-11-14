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
KINDS = {"rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base", "guard-v2", "reuse-hit", "reuse-miss"}
T = hp.HARNESS_PATHS

check("the table covers the candidate plans 41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56, 57a, 57b and no other", set(T) == {"41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56", "57a", "57b"}, sorted(T))
check("every path name is a known kind and none repeats", all(set(v) <= KINDS and len(set(v)) == len(v) for v in T.values()), T)
SINGLE_SEAT = ("56", "57a", "57b")       # one seat, suite reuse by the reference manifest
check("every plan has the rehearse, full, prior-tests and prior-tests-mismatch paths (plans 56, 57a, 57b: rehearse, full, reuse-hit and reuse-miss, their suite reuse being the reference manifest)",
      all({"rehearse", "full", "prior-tests", "prior-tests-mismatch"} <= set(v) for p, v in T.items() if p not in SINGLE_SEAT) and all({"rehearse", "full", "reuse-hit", "reuse-miss"} <= set(T[p]) for p in SINGLE_SEAT)
      and not any({"prior-tests", "prior-tests-mismatch"} & set(T[p]) for p in SINGLE_SEAT) and not any({"reuse-hit", "reuse-miss"} & set(v) for p, v in T.items() if p not in SINGLE_SEAT))
check("historical-base (LD_BASE_SHA unset) is listed for every plan that allows it, and not for plans 41d, 48, 52, 53, 54 and 54b, which require LD_BASE_SHA, nor 56, 57a, 57b (single seat: no BASE)",
      all(("historical-base" in v) == (p not in ("41d", "48", "52", "53", "54", "54b", "56", "57a", "57b")) for p, v in T.items()) and ': "${LD_BASE_SHA:?' in runall)
m = re.search(r'case "\$PLAN" in 41c\|41d\|46\|47\|48\|52\|53\|54b\|56\|57a\|57b\) ;; \*\) echo "ABORT-REHEARSE', runall)
check("loopdiag_run_all.sh defines LD_REHEARSE for exactly the plans of the table (other plans: ABORT-REHEARSE)", m is not None)
# plan 54 is plan 41d with LD_WITNESS54=1: run_all rewrites PLAN before the plan case, so it has no case line of its own, only the alias line
check("every plan of the table is selectable in run_all's plan case (plan 54 through its alias to plan 41d)", all(re.search(r"^  %s\) P_SMOKE=" % re.escape(p), runall, re.M) for p in T if p != "54")
      and 'W54=0; [ "$PLAN" = "54" ] && { W54=1; PLAN=41d; }' in runall and "54" in T)
check("LD_PRIOR_TESTS is wired into the suites step of the table's plans (suites_stage) and records its manifest at suite time",
      "suites_stage() {" in runall and "reuse_prior_tests() {" in runall and 'src_manifest > "$SD/SRC-MANIFEST.sha256"' in runall)
check("plan 56's suite reuse (suites_stage56) compares raptorpath/src + raptorpath/tests with the committed reference manifest, refuses LD_PRIOR_TESTS, and the tools tests (python and bash) run either way",
      "suites_stage56() {" in runall and 'find raptorpath/src raptorpath/tests -type f' in runall and "ref56-manifest.sha256" in runall and 'ABORT-TESTS plan 56 reuses the engine suites by the reference manifest' in runall
      and "tst bashtests" in runall and 'if [ "$ENGINE_SUITES" = 1 ]; then' in runall)

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
        d = os.path.join(td, "tools", "l1")
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
