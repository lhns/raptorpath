#!/usr/bin/env python3
"""Tests for status 63's harness (plan 63, F2's first VM contact: tools/l1/plan63.py, the l1common F2 witnesses, the place38_parse / perf_rwm_c.sh / battery / run_all wiring). The draft registration is the §63 draft;
the end-to-end launch paths run in test_stubrun.py (case63), the F2 helpers' own tests are in test_l1common.py.

  1. the table: 10 rows in CONTROL-then-CAND order, the arms are plan 51e's own (bytes, pace, timeouts), the smoke / rehearsal is the one CAND PL s42 row;
  2. the reported readings and labels against the draft's predictions (every branch);
  3. the row parser: only the seat's binary can void a row at plan 63; the F2 counters are readings there; at plans 51 / 51e a fail-closed row is VOID-F2, never a DNF datum;
  4. the shell side: plan accepted, two seats, the pins (LD_BIN_SHA_CAND required on a launch, the CONTROL pin default), the budget, ABORT classes, the env contract, the perf script's `--- F2` line;
  5. the engine-diff witness against a271e0bb (needs git and the commit: skipped in an archive).
Run: python3 test_harness63.py"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_NOGO", "LD_BIN_SHA_CAND", "LD_BIN_SHA_CONTROL", "LD_REHEARSE"):
    os.environ.pop(_k, None)
import harness_paths as hp  # noqa: E402
import l1common as lc  # noqa: E402
import plan51e  # noqa: E402
import plan63  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace").read()


runall, battery, perf = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh"), rd("perf_rwm_c.sh")
CELL = plan63.CELL

# ═══ 1. the table ═══
T = plan63.plan("63")
check("63 is 10 slots on c2-100: PL s42, PM s42, U s42, PL s7, PM s7, each CONTROL (C<arm>) then CAND (F<arm>); rep = the position, the smoke is the CAND PL s42 row, the rehearsal CONTROL PL s42 then CAND PL s42 (both reported, never re-run)",
      len(T) == 10 and [(r[2], r[3], r[4]) for r in T] == [("CPL", "42", 1), ("FPL", "42", 1), ("CPM", "42", 2), ("FPM", "42", 2), ("CU", "42", 3), ("FU", "42", 3), ("CPL", "7", 4), ("FPL", "7", 4), ("CPM", "7", 5), ("FPM", "7", 5)]
      and {r[0] for r in T} == {CELL} and plan63.plan("63", smoke=True) == [(CELL, "def", "FPL", "42", 1)] and plan63.plan("63", rehearsal=True) == [(CELL, "def", "CPL", "42", 1), (CELL, "def", "FPL", "42", 1)] and plan63.expect_rows() == 10, T)
check("the arms are plan 51e's own: PL 20 748 000 B at 2.7664 Mbit/s, PM 262 500 000 B at 35, U 600 000 000 B unpaced, the Realtime hint, RWM_SLAW=1 and RWM_C_RELIABLE=0, timeouts 75 / 75 / 30 s",
      all(plan63.arm_bytes(CELL, s + "PL") == 20_748_000 and plan63.arm_bytes(CELL, s + "PM") == 262_500_000 and plan63.arm_bytes(CELL, s + "U") == 600_000_000 for s in "CF")
      and plan63.arm_env(CELL, "FPL") == ["RWM_SLAW=1", "RWM_C_RELIABLE=0", "RWM_PERF_RATE_MBIT=2.7664"] and plan63.arm_env(CELL, "CPM") == ["RWM_SLAW=1", "RWM_C_RELIABLE=0", "RWM_PERF_RATE_MBIT=35"]
      and plan63.arm_env(CELL, "FU") == ["RWM_SLAW=1", "RWM_C_RELIABLE=0"] and [plan63.arm_timeout(CELL, a) for a in ("CPL", "FPM", "CU")] == [75, 75, 30] and plan63.arm_hint("CPL") == "realtime"
      and all(plan63.arm_env(CELL, s + b) == plan51e.arm_env(CELL, b) and plan63.arm_bytes(CELL, s + b) == plan51e.arm_bytes(CELL, b) for s in "CF" for b in ("PL", "PM", "U")))
check("armspec refuses an arm no plan carries (a bare plan-51e arm, a wrong cell), registered with the stage-3 parser as a realtime window arm on the BASE (C) / CAND (F) family",
      plan63.main(["armspec", CELL, "PL"]) == 1 and plan63.main(["armspec", "c8-100", "FPL"]) == 1 and plan63.main(["armspec", CELL, "FPL"]) == 0 and plan63.main(["plan", "64"]) == 3)
import place38_parse as pp  # noqa: E402
check("every arm is registered with place38_parse (window / Rlc / realtime) and its seat family (C -> BASE, F -> CAND)", all(pp.sp.ARM_SPEC[a][:3] == ("window", "Rlc", "realtime") for a in plan63.all_arms())
      and all(pp.AFAM[a] == ("BASE" if a.startswith("C") else "CAND") for a in plan63.all_arms()) and len(plan63.all_arms()) == 6)
check("the budget (section 63): plan63.BUDGET_S 5 400 s (1.5 h: two builds, the tools tests, 10 rows of at most 150 s) / 1 800 s (the rehearsal) and run_all's P57_NEED carries them", plan63.BUDGET_S == {"63": 5400, "63r": 1800} and 'P57_NEED=5400; [ "$REHEARSE" = 1 ] && P57_NEED=1800; P57_TAG="63"' in runall)

# ═══ 2. readings and labels ═══
def t63(warm="ACKED", und=0, unf=0, cli=((512, 512),), srv=((512, 512),), ctld=1, transfer="COMPLETED"):
    return {"warm": warm, "transfer": transfer, "undeliv": und, "ctld": ctld, "unframable": unf, "symsz": {"cli": list(cli), "srv": list(srv)}, "rate_echo": None}


check("CAND: PASS iff the warm-up acked, undeliv=0 from a PRESENT [CTLD] line, no [UNFRAMABLE], [SYMSZ] on both ends with local = decoder", plan63.cand_label(t63())[0] == "PASS")
check("CAND: transfer completion is reported SEPARATELY: a DNF / OTHER transfer after a clean warm-up and clean witnesses is still PASS (a pacer or E-tree finding, not one against F2)",
      plan63.cand_label(t63(transfer="DNF"))[0] == "PASS" and plan63.cand_label(t63(transfer="OTHER"))[0] == "PASS")
for name, t, frag in (("never acked", t63("NEVER-ACKED"), "NEVER-ACKED"), ("no client log", t63("NO-LOG"), "NO-LOG"), ("a MISSING [CTLD] line (no undeliv= anywhere): a missing witness, not a pass", t63(ctld=0), "missing witness"),
                      ("undeliv 2", t63(und=2), "undeliv=2"), ("an [UNFRAMABLE] line", t63(unf=1), "unframable=1"), ("no [SYMSZ] on the client", t63(cli=()), "missing on the client"),
                      ("no [SYMSZ] anywhere", t63(cli=(), srv=()), "both ends"), ("local != decoder", t63(srv=((1200, 512),)), "local != decoder")):
    lab, why = plan63.cand_label(t)
    check(f"CAND FAIL on {name}", lab == "FAIL" and any(frag in w for w in why), (lab, why))
check("CONTROL has NO per-row prediction (no pacer correlation; the failure is stochastic under one netem seed): every CONTROL row is REPORTED with its warm-up outcome, a failure is 'a datum, not an abort'",
      all(plan63.label_row(a, sd, t63(w))[0] == "REPORTED" for a in ("CPL", "CPM", "CU") for sd in ("42", "7") for w in ("ACKED", "NEVER-ACKED"))
      and "a datum, not an abort" in plan63.label_row("CPL", "42", t63("NEVER-ACKED"))[1] and "datum" not in plan63.label_row("CPL", "42", t63("ACKED"))[1])
check("label_row dispatches on the seat: F<arm> rows get the CAND's PASS / FAIL, C<arm> rows REPORTED", plan63.label_row("FPL", "42", t63())[0] == "PASS" and plan63.label_row("FPM", "7", t63(unf=1))[0] == "FAIL")
M = plan63.row_metrics(["[CTLD] undeliv=0", "[SYMSZ] local=512 decoder=512", '{"bytes":1,"mbps":3.0,"run":1}', '{"bytes":1,"mean_mbps":3.0,"summary":true}'], ["[SYMSZ] local=512 decoder=512"], ["--- F2 undeliv=0 unframable=0"])
check("row_metrics: a completed row reads warm=ACKED transfer=COMPLETED with its [SYMSZ] from both ends and one [CTLD] line carrying undeliv=", M["warm"] == "ACKED" and M["transfer"] == "COMPLETED" and M["ctld"] == 1
      and M["symsz"] == {"cli": [(512, 512)], "srv": [(512, 512)]} and M["undeliv"] == 0 and M["unframable"] == 0, M)
M = plan63.row_metrics(["x", '{"bytes":1,"dnf":0,"mean_mbps":3.0,"summary":true}'], [], [])
check("row_metrics: the real summary line carries `\"dnf\":0` on a completed run: still COMPLETED, while `\"dnf\":1` is a DNF transfer", M["transfer"] == "COMPLETED" and plan63.row_metrics(["x", '{"bytes":1,"dnf":1,"summary":true}'], [], [])["transfer"] == "DNF")
M = plan63.row_metrics(["Error: warm-up object never acked - tunnel not passing traffic", '{"dnf":true}'], ["[CTLD] undeliv=2", "[UNFRAMABLE] x"], ["--- F2 undeliv=2 unframable=1", '{"dnf":1,"summary":true}'])
check("row_metrics: 'warm-up object never acked' reads warm=NEVER-ACKED (the transfer is a separate DNF reading), undeliv 2 and unframable 1 are taken from both logs and the driver's `--- F2` line", M["warm"] == "NEVER-ACKED" and M["transfer"] == "DNF" and M["undeliv"] == 2 and M["unframable"] == 1, M)
M = plan63.row_metrics(["x", '{"dnf":true}'], [], [])
check("row_metrics: a DNF after an acked warm-up reads warm=ACKED transfer=DNF; no client log reads NO-LOG; a log with neither outcome reads transfer=OTHER", M["warm"] == "ACKED" and M["transfer"] == "DNF" and plan63.row_metrics([], [], [])["warm"] == "NO-LOG"
      and plan63.row_metrics(["x"], [], [])["transfer"] == "OTHER")
check("a [CTLD] line WITHOUT undeliv= (the CONTROL has no counter) is not a witness: ctld counts only the lines that carry a value; the driver's `--- F2 undeliv=0` line is not a [CTLD] line either",
      plan63.row_metrics(["[CTLD] t=5.0s gen=0"], ["[CTLD] t=5.0s gen=0"], ["--- F2 undeliv=0 unframable=0"])["ctld"] == 0 and plan63.row_metrics(["[CTLD] undeliv=0"], ["[CTLD] undeliv=0"], [])["ctld"] == 2)
check("row_problems: only the binary: a row on the wrong seat's sha is a bin-sha problem, a missing pin is 'uncheckable' (never a silent pass), an unknown arm is refused",
      plan63.row_problems(CELL, "CPL", [], [], "aa", "cc", "aa") == [] and plan63.row_problems(CELL, "FPL", [], [], "cc", "cc", "aa") == [] and any(x.startswith("bin-sha=") for x in plan63.row_problems(CELL, "CPL", [], [], "cc", "cc", "aa"))
      and any("uncheckable" in x for x in plan63.row_problems(CELL, "FPL", [], [], "cc", None, "aa")) and plan63.row_problems(CELL, "PL", [], [], "aa", "aa", "aa") == ["unknown-arm-PL-at-c2-100"])

# ═══ 3. the row parser ═══
td = tempfile.mkdtemp()


def parse_row(plan, arm, cli, srv, drv, sha="aa"):
    for n, ls in (("c.log", cli), ("s.log", srv), ("drv.out", drv)):
        with open(os.path.join(td, n), "w", newline="\n") as fh:
            fh.write("\n".join(ls) + "\n")
    env = dict(os.environ, LD_PLAN=plan, LD_SHA="aa", LD_SHA_BASE="bb", PYTHONUTF8="1")
    cp = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "row", CELL, arm, "42", "1", "0", "70", os.path.join(td, "drv.out"), os.path.join(td, "c.log"), os.path.join(td, "s.log"), "0", sha],
                        env=env, capture_output=True, text=True, timeout=120)
    line = next((ln for ln in cp.stdout.splitlines() if ln.startswith("P38ROW ")), None)
    return json.loads(line[len("P38ROW "):]) if line else {"error": cp.stdout[-400:] + cp.stderr[-400:]}


BAD_CLI = ["[CTLD] t=5.0s undeliv=2", '{"bytes":20748000,"dnf":true,"hint":"realtime","run":1}']
BAD_DRV = ['{"bytes":20748000,"dnf":1,"hint":"realtime","summary":true}', "--- F2 undeliv=2 unframable=1"]
r = parse_row("63", "FPL", BAD_CLI, ["[UNFRAMABLE] x"], BAD_DRV)
check("plan 63 parser: a fail-closed row (undeliv 2, [UNFRAMABLE], a DNF) is NOT voided: no VOID-F2 status, its F2 counters are readings in t63, the DNF stays a datum",
      r.get("status") != "VOID-F2" and not any(p.startswith("f2:") for p in r.get("problems", [])) and r["t63"]["undeliv"] == 2 and r["t63"]["unframable"] == 1 and r["dnf"] is True, r.get("error") or (r.get("status"), r.get("problems")))
check("plan 63 parser: a CONTROL row (C<arm>) is checked against LD_SHA_BASE, a CAND row against LD_SHA: the CAND sha on a CONTROL row is a bin-sha problem",
      any(p.startswith("bin-sha=") for p in parse_row("63", "CPL", BAD_CLI, [], BAD_DRV, sha="aa").get("problems", [])) and not any(p.startswith("bin-sha=") for p in parse_row("63", "CPL", BAD_CLI, [], BAD_DRV, sha="bb").get("problems", [])))
r = parse_row("51e", "PL", BAD_CLI, ["[UNFRAMABLE] x"], BAD_DRV)
check("plan 51e parser: the same fail-closed DNF row is VOID-F2, never a DNF datum (status VOID-F2, the reason in problems, dnf kept as a field)", r.get("status") == "VOID-F2" and any(p.startswith("f2:") and "undeliv=2" in p for p in r.get("problems", [])), r.get("error") or (r.get("status"), r.get("problems")))
r = parse_row("51e", "PL", ["[CTLD] t=5.0s undeliv=0", '{"bytes":20748000,"dnf":true,"hint":"realtime","run":1}'], [], ['{"bytes":20748000,"dnf":1,"summary":true}', "--- F2 undeliv=0 unframable=0"])
check("plan 51e parser: a DNF row WITHOUT the token is not VOID-F2 (an ordinary DNF datum)", r.get("status") != "VOID-F2" and not any(p.startswith("f2:") for p in r.get("problems", [])), r.get("error") or (r.get("status"), r.get("problems")))
r = parse_row("51", "HB", BAD_CLI, [], BAD_DRV)
check("plan 51 parser: VOID-F2 as well (block 51 gets the witness)", r.get("status") == "VOID-F2", r.get("error") or (r.get("status"), r.get("problems")))
check("the block scorers count it: s51_score and s51e_score print void_f2= in their VERDICT lines, and a VOID-F2 row is not live (RV.live needs status LIVE)",
      " void_f2={nf2}" in rd("s51e_score.py") and " void_f2={nf2}" in rd("s51_score.py"))
shutil.rmtree(td, ignore_errors=True)

# ═══ 4. the shell side ═══
check("perf_rwm_c.sh prints `--- F2 undeliv=<max over both logs> unframable=<count>` after the client exits (and the battery keeps it in the ledger: KEEP_PAT)",
      'echo "--- F2 undeliv=${_F2_UND:-0} unframable=${_F2_UNF:-0}"' in perf and "--- F2" in battery.split("KEEP_PAT='")[1].split("'")[0] and perf.index('echo "--- F2') > perf.index('[ "$CLI_ST" -ne 0 ] && echo "{\\"dnf\\":true'))
bash = shutil.which("bash")
if bash:
    snippet = "\n".join(ln for ln in perf.splitlines() if ln.startswith(("_F2_TXT=", "_F2_UND=", "_F2_UNF=", 'echo "--- F2')))
    d2 = tempfile.mkdtemp()
    try:
        with open(os.path.join(d2, "s.log"), "w", newline="\n") as fh:
            fh.write("\x1b[2m[CTLD]\x1b[0m t=1 undeliv=1\n[CTLD] t=2 undeliv=3\n[UNFRAMABLE] a\nplain\n")
        with open(os.path.join(d2, "c.log"), "w", newline="\n") as fh:
            fh.write("[CTLD] t=3 undeliv=2\n x [UNFRAMABLE] b\n")
        sn = snippet.replace("/tmp/rwm-s.log", "'%s'" % os.path.join(d2, "s.log").replace("\\", "/")).replace("/tmp/rwm-c.log", "'%s'" % os.path.join(d2, "c.log").replace("\\", "/"))
        cp = subprocess.run([bash, "-c", sn], capture_output=True, text=True, timeout=60)
        check("the perf script's F2 lines, run on logs with [CTLD] undeliv 1 / 3 / 2 and two [UNFRAMABLE] lines, print `--- F2 undeliv=3 unframable=2` (the maximum, the count) and l1common.f2_void reads it",
              cp.stdout.strip() == "--- F2 undeliv=3 unframable=2" and lc.f2_void(cp.stdout.splitlines()) is not None, (cp.stdout, cp.stderr[-200:]))
    finally:
        shutil.rmtree(d2, ignore_errors=True)
else:
    print("skip the perf script's F2 line on real logs: no bash here")
check("battery: plan 63 accepted (P63, single binary family per seat), the CONTROL seat is the BASE binary (LD_BIN_BASE / LD_SHA_BASE) for the C<arm> rows, a row runs ONCE (TRIES=1), nothing is replaced, plan 51e's scrub and instrument env ride",
      'P63=0; [ "$PLAN" = "63" ] && { P63=1; P38=1; PCAND=1; TRIES=1; }' in battery and 'case "$arm" in C*) RBIN="$BIN_BASE"; RWANT="$WANT_BASE" ;; esac' in battery and 'python3 ./plan63.py armspec "$cell" "$arm"' in battery
      and '[ "$P63" = 1 ] && INSTR_ENV=(RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=75)' in battery and "|| [ \"$P63\" = 1 ]; } && SCRUB_EXTRA=" in battery and "run_slot63" not in battery and "plan63.py replace" not in battery)
check("battery: the rehearsal is two rows (CONTROL PL s42, CAND PL s42), the smoke one", "ABORT-PLAN63 rehearsal table has" in battery and "plan63.py plan 63 rehearsal" in battery)
check("battery: both seats are preflighted (the E-block tokens), must be different files with different shas, and the CONTROL must equal LD_SHA_BASE; the table-size asserts are 10 / 1",
      'preflight_binary "$BIN_BASE" "RWM_SLAW" "RWM_UNIFIED_SHED" "unified overload shedding ACTIVE" "RWM_PERF_RATE_MBIT"' in battery and "ABORT-SHA start (CONTROL)" in battery and "ABORT-SHA CONTROL and CAND binaries are identical" in battery
      and 'want=10' in battery and "ABORT-PLAN63 smoke table has" in battery and "=== binary-base" in battery)
check("run_all: plan 63 is selectable and rehearsable, rides P46 + P57, and the CAND pin LD_BIN_SHA_CAND is REQUIRED on a launch (no default; the rehearsal records it) before any lock",
      "  63) P_SMOKE=" in runall and '[ "$PLAN" != "63" ]' in runall and 'P63=0; [ "$PLAN" = "63" ] && { P63=1; P46=1; P57=1; }' in runall
      and 'echo "ABORT-BIN-SHA plan 63 needs LD_BIN_SHA_CAND' in runall and runall.index("ABORT-BIN-SHA plan 63 needs") < runall.index("take_lock \"$LOCK_VM\"") and "|63)" in runall)
check("run_all: the CONTROL pin defaults to the registered E bin_sha 8013a312... (LD_BIN_SHA_CONTROL, a test seam) and equals plan63.CONTROL_SHA; both pins are exported for the report",
      "8013a3123aed0f4177a05efb15e4cf96117341bfaae743654f5221b434fc4caf" in runall and plan63.CONTROL_SHA == "8013a3123aed0f4177a05efb15e4cf96117341bfaae743654f5221b434fc4caf" and "export LD_BIN_SHA_CONTROL LD_BIN_SHA_CAND" in runall)
check("run_all: the CAND binary is checked against LD_BIN_SHA_CAND (ABORT-BIN-SHA ... CAND) or recorded, the CONTROL comes from LD_BIN_CONTROL (no build) or the BASE tree and is checked against its pin (ABORT-BIN-SHA ... CONTROL), "
      "a prebuilt CAND is refused (LD_BIN_PREBUILT), and no engine suite runs (no manifest ABORT)", 'fail_all "ABORT-BIN-SHA plan 63 CAND: built' in runall and 'fail_all "ABORT-BIN-SHA plan 63 CONTROL:' in runall and 'LD_BIN_CONTROL' in runall
      and 'if [ "$P60" = 1 ] && [ -n "${LD_BIN_PREBUILT:-}" ]; then' in runall and 'ABORT-BUILD plan 63 never accepts a prebuilt CAND' in runall and "plan 63: no engine suites" in runall)
check("run_all: the stage calls plan63.py for the smoke, the report (the score) and the slot check, the tools-test gate runs test_l1common.py and test_harness63.py, LD_HARD is required (budget guard launch + 1 h)",
      'python3 ./plan63.py smoke "$P51SMOKE"' in runall and 'python3 ./plan63.py report "$dir/ld.log"' in runall and 'python3 ./plan63.py check "$splan"' in runall and "for t in test_l1common.py test_harness63.py" in runall
      and "63) stage rep63 63" in runall)
check("the env-propagation contract: the new variables (LD_BIN_CONTROL, LD_BIN_SHA_CAND, LD_BIN_SHA_CONTROL) are read by run_all ONLY and never need to cross `sudo -n env`; what the battery and the row parser read of the seats "
      "(LD_BIN, LD_SHA, LD_BIN_BASE, LD_SHA_BASE) rides the stage's SENV", all(v not in battery for v in ("LD_BIN_CONTROL", "LD_BIN_SHA_CAND", "LD_BIN_SHA_CONTROL"))
      and all(f'{v}="$' in runall.split("local SENV=(")[1].split(")")[0] for v in ("LD_BIN", "LD_SHA", "LD_BIN_BASE", "LD_SHA_BASE")))
check("run_all and the battery guard the new files against CRLF", all(f in runall and f in battery for f in ("plan63.py", "test_harness63.py")))
check("HARNESS_PATHS lists 63 with rehearse, full, the ABORT paths (missing / wrong CAND pin, wrong CONTROL pin, a prebuilt CAND), cand-fail, cand-noctld, and neither prior-tests nor reuse paths", set(hp.HARNESS_PATHS["63"]) == {"rehearse", "full", "cand-sha-missing", "cand-sha-mismatch", "control-sha-mismatch", "cand-fail", "cand-noctld", "cand-prebuilt"})


# ═══ 5. the engine-diff witness ═══
def git(*a):
    try:
        return subprocess.run(["git", *a], cwd=os.path.join(HERE, "..", ".."), capture_output=True, text=True, timeout=60)
    except Exception:      # noqa: BLE001
        return None


BASE = "a271e0bb"
ENGINE = ("raptorpath", "raptorpath-math", "raptorpath-core", "raptorpath-wasm", "gf256", "Cargo.toml", "Cargo.lock")
g0 = git("rev-parse", "--git-dir")
g1 = git("cat-file", "-t", BASE) if g0 is not None and g0.returncode == 0 else None
if g1 is not None and g1.returncode == 0 and g1.stdout.strip() == "commit":
    dd = git("diff", "--stat", BASE, "HEAD", "--", *ENGINE)
    check(f"engine diff against {BASE} (raptorpath, raptorpath-math, raptorpath-core, raptorpath-wasm, gf256, Cargo.toml, Cargo.lock) is EMPTY", dd is not None and dd.returncode == 0 and dd.stdout.strip() == "", dd.stdout if dd else "git failed")
else:
    print(f"skip engine-diff witness: no git work tree or no commit {BASE} here (an archive); run test_harness63.py in the worktree")
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
