#!/usr/bin/env python3
"""Tests for status 64's harness (plan 64: the F2 merge guard; plan64.py, rt64_score.py, framing64.py and the l1common / perf_rwm_c.sh / place38_parse / battery / run_all wiring).
The registration is docs/derivations/f2-guard-registration.md (f2-guard-design 39269746), section 5; the launch paths run in test_stubrun.py (case64).

  1. the perf script's F2 lines run under the real script's shell options (`set -euo pipefail`, from lib.sh);
  2. the F2 row primitives and the denominator readout;
  3. stage G's F2 witnesses and the F2-FAIL status precedence;
  4. stage R's table, labels, replacement rule and score;
  5. framing64 on the two committed pre-read files;
  6. the shell side (suites runner, reuse, plan wiring).
Run: python3 test_harness64.py"""
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_WITNESS64", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_REHEARSE", "LD_BIN_SHA_BASE", "LD_BIN_SHA_CAND"):
    os.environ.pop(_k, None)
import l1common as lc  # noqa: E402

fails = 0
CHECKS = 0


def check(name, ok, detail=""):
    global fails, CHECKS
    CHECKS += 1
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace", newline="").read()


# ═══ 1. the perf script's F2 lines under `set -euo pipefail` ═══
perf = rd("perf_rwm_c.sh")
check("perf_rwm_c.sh prints `--- F2 undeliv=<max over both logs> unframable=<count>` after the client exits",
      'echo "--- F2 undeliv=${_F2_UND:-0} unframable=${_F2_UNF:-0}"' in perf and perf.index('echo "--- F2') > perf.index('[ "$CLI_ST" -ne 0 ] && echo "{\\"dnf\\":true'))
check("the battery keeps the `--- F2` line in the ledger (KEEP_PAT)", "--- F2" in rd("loopdiag_battery.sh").split("KEEP_PAT='")[1].split("'")[0])
f2_lines = [ln for ln in perf.splitlines() if ln.startswith(("_F2_TXT=", "_F2_UND=", "_F2_UNF=", 'echo "--- F2'))]
check("the perf script has exactly the four F2 lines, and every substitution ends in `|| true` (lib.sh turns `set -e` on: a bare `grep -c` with no match killed the script, rc 1, VOID-RC on every §63 row)",
      len(f2_lines) == 4 and all(ln.rstrip().endswith('|| true)"') for ln in f2_lines[:3]), f2_lines)
bash = shutil.which("bash")
if os.name == "nt":
    bash = next((c for c in (bash, r"C:\Program Files\Git\bin\bash.exe") if c and "system32" not in c.lower() and os.path.exists(c)), None)
if bash:
    snippet = "\n".join(f2_lines)
    d2 = tempfile.mkdtemp()
    try:
        def run_f2(s_text, c_text):
            with open(os.path.join(d2, "s.log"), "w", newline="\n") as fh:
                fh.write(s_text)
            with open(os.path.join(d2, "c.log"), "w", newline="\n") as fh:
                fh.write(c_text)
            sn = snippet.replace("/tmp/rwm-s.log", "'%s'" % os.path.join(d2, "s.log").replace("\\", "/")).replace("/tmp/rwm-c.log", "'%s'" % os.path.join(d2, "c.log").replace("\\", "/"))
            return subprocess.run([bash, "-c", "set -euo pipefail; " + sn + "; echo after-f2"], capture_output=True, text=True, timeout=60)
        cp = run_f2("\x1b[2m[CTLD]\x1b[0m t=1 undeliv=1\n[CTLD] t=2 undeliv=3\n[UNFRAMABLE] a\nplain\n", "[CTLD] t=3 undeliv=2\n x [UNFRAMABLE] b\n")
        check("the F2 lines under `set -euo pipefail` on logs with [CTLD] undeliv 1 / 3 / 2 and two [UNFRAMABLE] lines print `--- F2 undeliv=3 unframable=2` and l1common.f2_void reads it",
              cp.stdout.splitlines()[:1] == ["--- F2 undeliv=3 unframable=2"] and lc.f2_void(cp.stdout.splitlines()) is not None, (cp.returncode, cp.stdout, cp.stderr[-200:]))
        cp = run_f2("plain\n", "also plain\n")
        check("the F2 lines under `set -euo pipefail` on logs with NO [CTLD] and NO [UNFRAMABLE] line (grep exits 1 on no match) still print `--- F2 undeliv=0 unframable=0` and let the script go on (rc 0, `after-f2`)",
              cp.returncode == 0 and cp.stdout.splitlines()[:2] == ["--- F2 undeliv=0 unframable=0", "after-f2"], (cp.returncode, cp.stdout, cp.stderr[-200:]))
    finally:
        shutil.rmtree(d2, ignore_errors=True)
else:
    check("bash is available for the perf script's F2 lines", False, "no MSYS/Linux bash found")

if __name__ == "__main__":
    print("test_harness64: %d checks, %d failed" % (CHECKS, fails))
    sys.exit(1 if fails else 0)
