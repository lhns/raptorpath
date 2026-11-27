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
KINDS = {"rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base", "guard-v2", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "slaw-echo-void", "nogo-rehearse", "prebuilt", "rsign-abort", "cand-bulk-unframable", "cand-bulk-dnf-unframable", "cand-bulk-symsz-missing", "cand-bulk-ctld-send-only", "base-has-symsz", "completed-row-live", "base-rt-dead", "base-rt-dead-rc0", "rc-nodata-replaced", "rt-rho-one-end", "cand-rt-dnf-base-completes", "cand-rt-dnf-base-dnf", "cand-rt-ctld-send-only", "cand-rt-symsz-1200", "manifest-miss", "manifest-miss-cargo-toml-only", "rc-void-twice", "prebuilt-sha-mismatch", "tools-manifest-differs"} | {'rehearse-rb-dead-twice', 'replicated', 'base-dead-twice', 'unresolved-n', 'base-dead-redrawn', 'score-sha-mismatch', 'rehearse-rb-dead-once', 'rho-one-end', 'rc-rtfail', 'prebuilt-missing', 'prebuilt-sha-mismatch'} | {"pilot-missing", "control-missing", "reprobe", "nogo-floor", "nogo-gate", "nogo-pilot", "nogo-control", "nogo-short", "abort-netem-first", "prebuilt-required", "budget", "tools-reuse-miss", "rehearsal-missing", "rehearsal-nogo", "knee-mismatch", "abort-netem-last"} | {"pace-void", "spc-void", "calibrate", "recheck", "delta-echo-void", "short-void", "dialpins-fail", "cand-sha-missing", "cand-sha-mismatch", "control-sha-mismatch", "cand-fail", "cand-noctld", "cand-ctld-send-only", "cand-prebuilt"}
T = hp.HARNESS_PATHS
KINDS |= {v for p_, vs in T.items() if p_ in ("67", "67g", "67x") for v in vs}      # status 67: its own path names (test_stubrun.case67 runs each)
KINDS |= {v for p_, vs in T.items() if p_ in ("68", "68r") for v in vs}      # status 68: its own path names (test_stubrun.case68 runs each)
S67 = ("67", "67g", "67x")      # seven seats, three session tokens: the rehearsal and the two launches; no prior-tests / historical-base paths

check("the table covers the candidate plans 41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56, 57a, 57b, 59, 60a, 60b, 61a, 61b, 62r0, 62a, 51e, 51, 63, 64, 65, 67, 67g, 67x, 68, 68r and no other", set(T) == {"68r", "67", "67g", "67x", "41c", "41d", "46", "47", "48", "52", "53", "54", "54b", "56", "57a", "57b", "59", "60a", "60b", "61a", "61b", "62r0", "62a", "51e", "51", "63", "64", "65", "68"}, sorted(T))
check("every path name is a known kind and none repeats", all(set(v) <= KINDS and len(set(v)) == len(v) for v in T.values()), T)
SINGLE_SEAT = ("56", "57a", "57b", "59")       # one seat, suite reuse by the reference manifest
S60 = ("60a", "60b", "61a", "61b", "51e")          # status 60 / 61 / 51e: one seat, the manifest rule is strict (a MISS aborts), so no "full" path
S62 = ("62r0", "62a")                          # status 62: the 61 shape, no build; its own path set (62r0: the rehearsal and its NO-GOs; 62a: the cell and the tools-test reuse)
S51 = ("51", "63", "68", "68r")                           # status 51 block 1 / status 63: main at launch, no engine suites at all (no prior-tests, no reuse paths); rehearse and full
S65 = ("65",)                                 # status 65: stage R only on the prebuilt binaries of status 64: no suites, no BASE tree, no LD_BASE_SHA; "full" is the launch on the rehearsal's tools result
S64 = ("64",)                                 # status 64: two seats, the manifest rule is strict (a MISS aborts) and a "full" path is the launch shape (prebuilt binaries + the rehearsal's tools result); no prior-tests paths
check("every plan has the rehearse, full, prior-tests and prior-tests-mismatch paths (plans 56, 57a, 57b, 59: rehearse, full, reuse-hit and reuse-miss, their suite reuse being the reference manifest)",
      all({"rehearse", "full", "prior-tests", "prior-tests-mismatch"} <= set(v) for p, v in T.items() if p not in SINGLE_SEAT + S60 + S62 + S64 + S65 + S51 + S67) and all({"rehearse", "full", "reuse-hit", "reuse-miss"} <= set(T[p]) for p in SINGLE_SEAT)
      and not any({"prior-tests", "prior-tests-mismatch"} & set(T[p]) for p in SINGLE_SEAT) and not any({"reuse-hit", "reuse-miss"} & set(v) for p, v in T.items() if p not in SINGLE_SEAT + S60 + S62 + S64 + S65 + S51 + S67)
      and all({"rehearse", "full", "manifest-miss", "prebuilt-sha-mismatch", "tools-manifest-differs"} <= set(T[p]) and not ({"prior-tests", "prior-tests-mismatch", "reuse-hit", "reuse-miss", "historical-base"} & set(T[p])) for p in S64) and all({"rehearse", "full", "rehearse-rb-dead-twice", "score-sha-mismatch", "prebuilt-missing"} <= set(T[p]) and not ({"prior-tests", "prior-tests-mismatch", "reuse-hit", "reuse-miss", "historical-base"} & set(T[p])) for p in S65)
      and all({"rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch"} <= set(T[p]) and "full" not in T[p] and not ({"prior-tests", "prior-tests-mismatch"} & set(T[p])) for p in S60 if p != "61b")
      and all({"rehearse", "full"} <= set(T[p]) and not ({"prior-tests", "prior-tests-mismatch", "reuse-hit", "reuse-miss"} & set(T[p])) for p in S51)
      and {"reuse-hit", "reuse-miss", "bin-sha-mismatch", "prebuilt"} == set(T["61b"])      # 61b has no rehearsal of its own (s61 registration section 6: c2's rows ran in s60 on this binary)
      and {"rehearse", "reprobe", "nogo-floor", "nogo-gate", "nogo-pilot", "nogo-control", "nogo-short", "abort-netem-first", "control-missing", "prebuilt-required", "reuse-miss", "bin-sha-mismatch", "budget"} == set(T["62r0"])
      and {"full", "tools-reuse-miss", "rehearsal-missing", "rehearsal-nogo", "knee-mismatch", "pilot-missing", "abort-netem-last", "slaw-echo-void", "budget"} == set(T["62a"]))      # plans 62r0 / 62a: their own path sets
check("historical-base (LD_BASE_SHA unset) is listed for every plan that allows it, and not for plans 41d, 48, 52, 53, 54 and 54b, which require LD_BASE_SHA, nor 56, 57a, 57b, 59, 60a, 60b (single seat: no BASE)",
      all(("historical-base" in v) == (p not in ("41d", "48", "52", "53", "54", "54b", "56", "57a", "57b", "59", "60a", "60b", "61a", "61b", "62r0", "62a", "51e", "51", "63", "64", "65", "68", "68r") + S67) for p, v in T.items()) and ': "${LD_BASE_SHA:?' in runall)
m = re.search(r'case "\$PLAN" in 62r0\|61a\) ;; 41c\|41d\|46\|47\|48\|52\|53\|54b\|56\|57a\|57b\|59\|60a\|60b\|65\|67\) ;; \*\) echo "ABORT-REHEARSE', runall)
check("loopdiag_run_all.sh defines LD_REHEARSE for exactly the plans of the table (other plans: ABORT-REHEARSE)", m is not None)
# plan 54 is plan 41d with LD_WITNESS54=1: run_all rewrites PLAN before the plan case, so it has no case line of its own, only the alias line
check("every plan of the table is selectable in run_all's plan case (plan 54 through its alias to plan 41d)", all(re.search(r"^  (?:\w+\|)*%s(?:\|\w+)*\) P_SMOKE=" % re.escape(p), runall, re.M) for p in T if p not in ("54", "64"))
      and 'W54=0; [ "$PLAN" = "54" ] && { W54=1; PLAN=41d; }' in runall and 'W64=0; [ "$PLAN" = "64" ] && { W64=1; PLAN=41d; }' in runall and "54" in T and "64" in T)
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
      man is not None and all(x in man.group(0) for x in ("raptorpath/src", "raptorpath/tests", "raptorpath-math/src", "raptorpath-core/src", "raptorpath-wasm/src", "gf256/src", "vendor", "Cargo.toml", "Cargo.lock", "./*/Cargo.toml")), man and man.group(0))
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
