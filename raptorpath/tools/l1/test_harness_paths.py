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

check("the table covers the candidate plans 41c, 46, 47, 48 and no other", set(T) == {"41c", "46", "47", "48"}, sorted(T))
check("every path name is a known kind and none repeats", all(set(v) <= KINDS and len(set(v)) == len(v) for v in T.values()), T)
check("every plan has the rehearse, full, prior-tests and prior-tests-mismatch paths", all({"rehearse", "full", "prior-tests", "prior-tests-mismatch"} <= set(v) for v in T.values()))
check("historical-base (LD_BASE_SHA unset) is listed for every plan that allows it, and not for plan 48, which requires LD_BASE_SHA",
      all(("historical-base" in v) == (p != "48") for p, v in T.items()) and ': "${LD_BASE_SHA:?' in runall)
m = re.search(r'case "\$PLAN" in 41c\|46\|47\|48\) ;; \*\) echo "ABORT-REHEARSE', runall)
check("loopdiag_run_all.sh defines LD_REHEARSE for exactly the plans of the table (other plans: ABORT-REHEARSE)", m is not None)
check("every plan of the table is selectable in run_all's plan case", all(re.search(r"^  %s\) P_SMOKE=" % re.escape(p), runall, re.M) for p in T))
check("LD_PRIOR_TESTS is wired into the suites step of the table's plans (suites_stage) and records its manifest at suite time",
      "suites_stage() {" in runall and "reuse_prior_tests() {" in runall and 'src_manifest > "$SD/SRC-MANIFEST.sha256"' in runall)
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
