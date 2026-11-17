#!/usr/bin/env python3
"""Tests for status 61's harness (plans 61a / 61b, the c3 sweep: tools/l1/plan61.py, the place38_parse hook, the battery / run_all wiring). The registration is docs/derivations/s61-registration.md;
the scorer's own tests are test_s61.py and the end-to-end launch paths run in test_stubrun.py.

  1. the tables: 6 / 66 / 30 / 1 + 1 rows, the Latin square, seed alternation, the arm derivation at W-hat 64 / 115 / 192, ABORT-PILOT without LD_KNEE61, the control fallback;
  2. the per-arm run spec: bytes (the formula, 120 MB at the knee bracket and the control, 600 MB at c2), timeouts, hint, env (RWM_SLAW first, the pool knob not at 2 048);
  3. the row witnesses and the replacement rule (echo VOID once, c3 floor DNF once, a pilot DNF only in the rehearsal, never a wrong binary);
  4. the shell side: plan accepted, the P61 branch, the table-size asserts, the registered bin_sha, ABORT-PILOT / NOGO-CONTROL, the budget, the stage's scorer calls, the CRLF guard lists;
  5. the engine-diff witness (section 7): the engine crates and ref60-manifest.sha256 equal 1ecee59d's (needs git and the commit: skipped in an archive).
Run: python3 test_harness61.py"""
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_NOGO", "LD_KNEE61", "LD_CTL61", "LD_REHEARSE"):
    os.environ.pop(_k, None)
import harness_paths as hp  # noqa: E402
import place38_parse as pp  # noqa: E402
import plan61  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace").read()


runall, battery = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh")
C3, C2 = plan61.C3, plan61.C2

# ═══ 1. tables ═══
R = plan61.plan("61r")
os.environ["LD_KNEE61"] = "115"
A, B = plan61.plan("61a"), plan61.plan("61b")
check("61a is 66 slots (c3: 11 arms x 6 replicates), 61b is 30 (c2: 5 arms x 6), the rehearsal 6, the smokes 1 each",
      (len(A), len(B), len(R), len(plan61.plan("61a", smoke=True)), len(plan61.plan("61b", smoke=True))) == (66, 30, 6, 1, 1), (len(A), len(B), len(R)))
check("61a: every slot is c3-25, 6 replicates of each of the 11 arms (floor 12 / 16 / 24, B x3, K x3, P1, control 2 048), no duplicate slot",
      {r[0] for r in A} == {C3} and all(sum(1 for r in A if r[2] == a) == 6 for a in ["G%d" % w for w in plan61.c3_grid(115.0)]) and len({(r[0], r[2], r[3], r[4]) for r in A}) == 66 and len({r[2] for r in A}) == 11)
check("61a: the arms rotate by replicate (a cyclic Latin square: replicate k starts at arm k mod 11) and the seeds alternate 42 / 7",
      [r[2] for r in A[:3]] == ["G12", "G16", "G24"] and A[11][2] == "G16" and A[22][2] == "G24" and [A[11 * k][3] for k in range(6)] == ["42", "7", "42", "7", "42", "7"] and [A[11 * k][4] for k in range(6)] == [1, 2, 3, 4, 5, 6])
check("61b: every slot is c2-100, arms 96 / 128 / 144 / 160 / 2 048 x 6, rotated like 61a", {r[0] for r in B} == {C2} and [r[2] for r in B[:5]] == ["G96", "G128", "G144", "G160", "G2048"] and B[5][2] == "G128" and len({(r[2], r[3], r[4]) for r in B}) == 30)
check("the rehearsal is R1 G2 048, R2 / R3 G12 (seeds 42 and 7), R4 / R5 G88 / G128 (the pilot), R6 G512 (the fallback control), every slot unique", [r[2] for r in R] == ["G2048", "G12", "G12", "G88", "G128", "G512"]
      and [r[3] for r in R[1:3]] == ["42", "7"] and len({(r[0], r[2], r[3], r[4]) for r in R}) == 6 and plan61.plan("61a", rehearsal=True) == R)
check("the smokes: 61a = c3 G at K2 (W-hat 115 -> G120), 61b = c2 G160", plan61.plan("61a", smoke=True) == [(C3, "def", "G120", "42", 1)] and plan61.plan("61b", smoke=True) == [(C2, "def", "G160", "42", 1)])
for w_hat, want in ((64, {"K": [60, 68, 72], "B": [36, 48, 56], "P1": 96}), (115, {"K": [108, 120, 132], "B": [64, 88, 100], "P1": 176}), (192, {"K": [184, 200, 220], "B": [108, 144, 164], "P1": 288})):
    check(f"the arms at W-hat {w_hat}: K = r4(0.95 / 1.05 / 1.14 W-hat), B = r4(0.57 / 0.76 / 0.86 W-hat), P1 = r8(1.5 W-hat) = {want}", plan61.arms(w_hat) == want, plan61.arms(w_hat))
check("W-hat is clamped to [64, 192] and r4 rounds half up", plan61.arms(10)["K"] == plan61.arms(64)["K"] and plan61.arms(999)["P1"] == plan61.arms(192)["P1"] and plan61.r_to(62, 4) == 64 and plan61.r_to(61.9, 4) == 60)
check("the 11 c3 arms are distinct for every W-hat in [64, 192] (0.25 steps) and never collide with a floor pool or the control", all(len(set(plan61.c3_grid(k / 4.0))) == 11 for k in range(256, 769)))
del os.environ["LD_KNEE61"]
try:
    plan61.plan("61a")
    raised = False
except ValueError:
    raised = True
check("ABORT-PILOT: 61a without LD_KNEE61 raises (the CLI prints ABORT-PILOT, rc 3); the rehearsal and 61b need none", raised and plan61.main(["plan", "61a"]) == 3 and plan61.main(["count", "61a", "smoke"]) == 3
      and len(plan61.plan("61r")) == 6 and len(plan61.plan("61b")) == 30)
os.environ["LD_KNEE61"] = "300"
check("an out-of-range LD_KNEE61 (300 > 192) is refused, not clamped: the launch must not run on a guess", plan61.main(["plan", "61a", "smoke"]) == 3)
os.environ["LD_KNEE61"] = "abc"
check("an unparsable LD_KNEE61 is refused (ABORT-PILOT, rc 3)", plan61.main(["plan", "61a", "smoke"]) == 3)
os.environ["LD_KNEE61"] = "115"
os.environ["LD_CTL61"] = "512"
A512 = plan61.plan("61a")
check("LD_CTL61=512 swaps the control arm: G512 replaces G2 048 in every replicate (66 slots, 11 arms); 61b keeps 2 048", {r[2] for r in A512} >= {"G512"} and "G2048" not in {r[2] for r in A512} and len(A512) == 66 and "G2048" in {r[2] for r in plan61.plan("61b")})
B512 = plan61.plan("61b")
del os.environ["LD_CTL61"]
B_DEF = plan61.plan("61b")
check("status 61 Amendment 2: the GLOBAL default is not flipped: `plan61.py plan 61b` prints the same 30 rows with and without LD_CTL61=512 (control W 2 048), and 61a's default control is still W 2 048 when the variable is unset",
      B512 == B_DEF and len(B_DEF) == 30 and sum(1 for r in B_DEF if r[2] == "G2048") == 6 and not any(r[2] == "G512" for r in B_DEF) and plan61.control_w() == 2048 and plan61.control_w("512") == 512
      and any(r[2] == "G2048" for r in plan61.plan("61a")) and any(r[2] == "G512" for r in plan61.plan("61a", ctl="512")) and not any(r[2] == "G2048" for r in plan61.plan("61a", ctl="512")))
del os.environ["LD_KNEE61"]
plan61.REPS_CAP61 = 2
os.environ["LD_KNEE61"] = "115"
check("the replicate cap (the stub suite's fast mode) shrinks both plans and keeps the structure", len(plan61.plan("61a")) == 22 and len(plan61.plan("61b")) == 10, (len(plan61.plan("61a")), len(plan61.plan("61b"))))
plan61.REPS_CAP61 = None
check("every arm of every table is registered with the stage-3 parser (window / Rlc / bulk) and rides the candidate seat",
      all(pp.sp.ARM_SPEC[r[2]][:3] == ("window", "Rlc", "bulk") and pp.AFAM[r[2]] == "CAND" for r in A + B + R))
check("the pilot rule's CLI prints PILOT61 and the arms (u 0.6 / 0.85 -> 144)", plan61.main(["pilot", "0.6", "0.85"]) == 0)

# ═══ 2. per-arm spec ═══
spec = {(c, a): (plan61.arm_bytes(c, a), plan61.arm_timeout(c, a), plan61.arm_hint(a), plan61.arm_env(c, a)) for c, _, a, _, _ in A + B + R}
check("floor-pool bytes: the formula 53 s * min(0.19 W, 17.5) Mbit/s / 8 to the nearest 5 MB = 15 / 20 / 30 MB at W 12 / 16 / 24", [spec[(C3, "G%d" % w)][0] for w in (12, 16, 24)] == [15_000_000, 20_000_000, 30_000_000]
      and [plan61.formula_mb(w) for w in (12, 16, 24)] == [15, 20, 30])
check("at W-hat 115: 120 MB at and above K1 (108), at P1 and at the control (2 048); the below-knee arms take the formula (64 -> 80, 88 -> 110, 100 -> 115 MB: <= 120, transfer <= 71 s at the slow basis)",
      all(spec[(C3, "G%d" % w)][0] == 120_000_000 for w in (108, 120, 132, 176, 2048)) and [spec[(C3, "G%d" % w)][0] for w in (64, 88, 100)] == [80_000_000, 110_000_000, 115_000_000])
check("with no pilot knee (the rehearsal) no row is 'at the knee bracket': the pilot rows take the formula (110 / 115 MB), the controls 120 MB, the floor rows 15 MB",
      [plan61.arm_bytes(C3, a, knee=None) for a in ("G88", "G128", "G2048", "G512", "G12")] == [110_000_000, 115_000_000, 120_000_000, 120_000_000, 15_000_000])
check("c2 (61b): 600 MB on every row", all(spec[(C2, a)][0] == 600_000_000 for a in ("G96", "G128", "G144", "G160", "G2048")))
check("timeouts: 120 s on every c3 row, 150 s on every c2 row", all(v[1] == (150 if k[0] == C2 else 120) for k, v in spec.items()))
check("hint: bulk everywhere", all(v[2] == "bulk" for v in spec.values()))
check("env: RWM_SLAW=1 first, RWM_RTOBS=1, then the pool knob RWM_STORE_PATH_POOL = W (one live path), NOT set at the shipped 2 048",
      spec[(C3, "G12")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=12"] and spec[(C3, "G2048")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1"] and spec[(C3, "G512")][3][-1] == "RWM_STORE_PATH_POOL=512"
      and spec[(C2, "G96")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=96"] and spec[(C2, "G2048")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1"])
check("an unknown (cell, arm) is refused by armspec (rc 1): a C arm, an arm no plan carries, an arm at the wrong cell", plan61.main(["armspec", C3, "G999"]) == 1 and plan61.main(["armspec", C3, "C12"]) == 1 and plan61.main(["armspec", C2, "G12"]) == 1
      and plan61.main(["armspec", "c8-100", "G12"]) == 1 and plan61.main(["armspec", C3, "G12"]) == 0)

# ═══ 3. witnesses and the replacement rule ═══
G = "[GATES] RWM_DIAG=1 RWM_THREE_TERM=0 RWM_STORE_PATH_POOL=%s RWM_SLAW=%s"
POOLINFO = "path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS) pool_per_path=%s"
SHA = "a" * 64


def logs(slaw_c="1", slaw_s="1", pool="12", diag="1"):
    return ([G.replace("RWM_DIAG=1", "RWM_DIAG=" + diag) % (pool, slaw_c), POOLINFO % pool], [G % (pool, slaw_s), POOLINFO % pool])


c, s = logs()
check("a G row echoing RWM_SLAW=1, RWM_DIAG=1, RWM_THREE_TERM=0 and its pool on both ends has no problem", plan61.row_problems(C3, "G12", c, s, SHA, SHA) == [], plan61.row_problems(C3, "G12", c, s, SHA, SHA))
check("RWM_SLAW echoed 0 on the client end is a gate-slaw problem (VOID); echoed 1 only on the server end of a 0 / 1 pair likewise",
      any(x.startswith("gate-slaw-cli=0") for x in plan61.row_problems(C3, "G12", *logs(slaw_c="0"), SHA, SHA)) and any(x.startswith("gate-slaw-srv=0") for x in plan61.row_problems(C3, "G12", *logs(slaw_s="0"), SHA, SHA)))
check("RWM_DIAG not 1 and a wrong pool echo are problems; a foreign binary is a bin-sha problem; an arm no plan carries is refused",
      any(x.startswith("gate-diag") for x in plan61.row_problems(C3, "G12", *logs(diag="0"), SHA, SHA)) and any(x.startswith("gate-pool") for x in plan61.row_problems(C3, "G12", *logs(pool="2048"), SHA, SHA))
      and any(x.startswith("bin-sha") for x in plan61.row_problems(C3, "G12", *logs(), SHA, "b" * 64)) and plan61.row_problems(C3, "G13", *logs(), SHA, SHA) == ["unknown-arm-G13-at-c3-25"])
check("the c3 control echoes the shipped pool 2 048, the c2 G96 echoes 96", plan61.row_problems(C3, "G2048", *logs(pool="2048"), SHA, SHA) == [] and plan61.row_problems(C2, "G96", *logs(pool="96"), SHA, SHA) == [])
d = tempfile.mkdtemp()


def ledger(rows):
    p = os.path.join(d, "ld%d.log" % len(os.listdir(d)))
    with open(p, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    return p


def row(cell, arm, seed, rep, problems=(), dnf=False, status="LIVE"):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "problems": list(problems), "dnf": dnf, "status": status}


check("replacement: a gate-slaw VOID slot is re-run once ('slaw-echo'); a second row for the slot stops it",
      plan61.replace_reason(ledger([row(C3, "G12", "42", 1, ["gate-slaw-cli=0 (want 1)"], status="WITNESS-FAIL")]), C3, "G12", "42", 1) == "slaw-echo"
      and plan61.replace_reason(ledger([row(C3, "G12", "42", 1, ["gate-slaw-cli=0 (want 1)"], status="WITNESS-FAIL"), row(C3, "G12", "42", 1)]), C3, "G12", "42", 1) is None)
check("replacement: a pool-echo VOID ('pool-echo'), a c3 floor-pool DNF ('floor-dnf'); a non-floor DNF, a LIVE row, a crash row (VOID-ONDISK) and a c2 DNF are NOT replaced",
      plan61.replace_reason(ledger([row(C3, "G16", "7", 2, ["gate-pool-cli=2048 (want 16)"], status="WITNESS-FAIL")]), C3, "G16", "7", 2) == "pool-echo"
      and plan61.replace_reason(ledger([row(C3, "G24", "42", 1, dnf=True)]), C3, "G24", "42", 1) == "floor-dnf"
      and plan61.replace_reason(ledger([row(C3, "G108", "42", 1, dnf=True)]), C3, "G108", "42", 1) is None and plan61.replace_reason(ledger([row(C3, "G12", "42", 1)]), C3, "G12", "42", 1) is None
      and plan61.replace_reason(ledger([row(C3, "G12", "42", 1, dnf=True, status="VOID-ONDISK")]), C3, "G12", "42", 1) is None and plan61.replace_reason(ledger([row(C2, "G96", "42", 1, dnf=True)]), C2, "G96", "42", 1) is None)
check("replacement: a WRONG BINARY (bin-sha-*) never re-runs", plan61.replace_reason(ledger([row(C3, "G12", "42", 1, ["bin-sha=aaa (want bbb for CAND)"], status="WITNESS-FAIL")]), C3, "G12", "42", 1) is None)
os.environ["LD_REHEARSE"] = "1"
check("a pilot-row DNF (W 88 / 128) re-runs once in the REHEARSAL only; the same DNF in a launch is an ordinary arm's datum",
      plan61.replace_reason(ledger([row(C3, "G88", "42", 1, dnf=True)]), C3, "G88", "42", 1) == "pilot-dnf")
del os.environ["LD_REHEARSE"]
check("... and not outside it", plan61.replace_reason(ledger([row(C3, "G88", "42", 1, dnf=True)]), C3, "G88", "42", 1) is None)
os.environ["LD_KNEE61"] = "115"
rows_ok = [row(c_, a_, sd, rp) for c_, _, a_, sd, rp in A]
p_ok = ledger(rows_ok)
check("plan61.py check: rc 1 without LD-BATTERY-DONE, rc 0 with it and a row for every slot (a replaced slot's second row is not an extra slot)", plan61.check("61a", p_ok) == 1 and (open(p_ok, "a").write("LD-BATTERY-DONE tag=ld blocks=1\n") or plan61.check("61a", p_ok) == 0))
check("budget: plan61.BUDGET_S = 14 220 s (61a: 3.95 h) / 6 000 s (61b: 1.67 h) and run_all's P57_NEED carries them", plan61.BUDGET_S == {"61a": 14220, "61b": 6000} and 'P57_NEED=14220; [ "$PLAN" = "61b" ] && P57_NEED=6000' in runall
      and plan61.FLOOR_RERUN_S == 160 and 66 * 160 + 18 * 160 == 13440)
del os.environ["LD_KNEE61"]

# ═══ 4. shell side ═══
check("battery: plans 61a / 61b accepted (P61), single-seat (P38 + PCAND, no P41), the instrument env is RWM_RDIAG=1 + the timeout, the cell c3-25 has its cell_spec",
      'P61=0; case "$PLAN" in 61a|61b) P61=1 ;; esac' in battery and '[ "$P61" = 1 ] && { P38=1; PCAND=1; }' in battery and '[ "$P61" = 1 ] && INSTR_ENV=(RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=120)' in battery
      and 'c3-25)   echo "c3 c3 single 25000000"' in battery)
check("battery: every row's bytes / timeout / hint / env come from plan61.py armspec (one table), an unknown arm is refused, the cell must be the plan's, HINT is per row", 'python3 ./plan61.py armspec "$cell" "$arm"' in battery
      and 'read -r bytes p61_tmo HINT p60_arm_env' in battery and 'tmo_env=(RWM_PERF_TIMEOUT_S="$p61_tmo")' in battery and "UNKNOWN-ARM $arm at $cell (plan 61)" in battery and 'UNKNOWN-CELL $cell (plan $PLAN)' in battery
      and 'if [ "$P60" = 1 ] || [ "$P61" = 1 ]; then\n    read -r -a arm_env <<< "$p60_arm_env"' in battery)
check("battery: the table-size asserts (66 / 30, 6 rehearsal, 1 smoke), ABORT-PILOT without LD_KNEE61, the replacement call, the binary must carry the gauge tokens",
      'want=66; [ "$PLAN" = "61b" ] && want=30' in battery and '-eq 6 ]' in battery and "ABORT-PILOT" in battery and "python3 ./plan61.py replace" in battery and 'if [ "$P60" = 1 ] || [ "$P61" = 1 ]; then\n    preflight_binary "$BIN" "RWM_SLAW"' in battery)
check("battery: the 6 rehearsal rows run through the replacement rule (a pilot / floor DNF re-runs once), the smoke row of a launch through run_one", 'run_slot61 "$cell" "$arm" "$seed" "$rep" "$store"' in battery and 'run_one "$cell" "$arm" "$seed" "$rep" "$store"; else run_slot61' in battery)
check("run_all: plans 61a / 61b are selectable, 61a rehearsable (61b is not: ABORT-REHEARSE), both ride plan 60's gates (P60: bin_sha, manifest; P57: budget, /dev/shm; P46) and the R-sign gate stays 60b's",
      "  61a) P_SMOKE=" in runall and "  61b) P_SMOKE=" in runall and 'case "$PLAN" in 61a) ;; 41c|' in runall and 'P61=0; { [ "$PLAN" = "61a" ] || [ "$PLAN" = "61b" ]; } && { P61=1; P60=1; P46=1; P57=1; }' in runall
      and '[ "$PLAN" = "60b" ] && [ "$REHEARSE" != "1" ]' in runall)
check("run_all: a 61a launch, the rehearsal included, refuses unless LD_CTL61=512 (ABORT-CONTROL61, nothing run), inside the P61 block after the bin_sha check; plan 61b is not subject to it",
      'if [ "$PLAN" = "61a" ] && [ "${LD_CTL61:-}" != "512" ]; then\n    fail_all "ABORT-CONTROL61 plan 61a' in runall and runall.index("ABORT-CONTROL61") > runall.index('fail_all "ABORT-BIN-SHA plan $PLAN') and runall.index("ABORT-CONTROL61") < runall.index('fail_all "ABORT-PILOT plan 61a'))
check("run_all: the registered bin_sha is checked before any row for the 61 plans too (they ride P60: ABORT-BIN-SHA), the toolchain is logged, then ABORT-PILOT (no / invalid LD_KNEE61) and NOGO-CONTROL (LD_NOGO=control), nothing run",
      'if [ "$P60" = 1 ] && [ "$SHA" != "$LD_BIN_SHA_EXPECT" ]; then' in runall and "LD-ALL toolchain" in runall and 'fail_all "ABORT-PILOT plan 61a' in runall and 'fail_all "NOGO-CONTROL plan 61a' in runall
      and "e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d" in runall)
check("run_all: the tools-test gate runs test_s61.py and test_harness61.py; the stage calls the 61 scorer for the smoke (the 6 rows + go/no-go on the rehearsal), the score and the slot check; LD_KNEE61 / LD_CTL61 cross `sudo -n env`",
      "for t in test_s61.py test_harness61.py" in runall and 's61_score.py smoke61 "$P61SMOKE"' in runall and "P61SMOKE=61r" in runall and 's61_score.py score61 "$splan"' in runall and 'plan61.py check "$splan"' in runall
      and 'LD_KNEE61="${LD_KNEE61:-}" LD_CTL61="${LD_CTL61:-}"' in runall and "61a) stage rep61a 61a" in runall and "61b) stage rep61b 61b" in runall)
check("run_all and the battery guard the new files against CRLF", all(f in runall for f in ("plan61.py", "s61_score.py", "test_s61.py", "test_harness61.py")) and all(f in battery for f in ("plan61.py", "s61_score.py", "test_s61.py", "test_harness61.py")))
check("HARNESS_PATHS lists 61a (with rehearse, slaw-echo-void, pilot-missing) and 61b (prebuilt, no rehearse); neither has 'full'", {"rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "slaw-echo-void", "pilot-missing", "control-missing"} == set(hp.HARNESS_PATHS["61a"])
      and {"reuse-hit", "reuse-miss", "bin-sha-mismatch", "prebuilt"} == set(hp.HARNESS_PATHS["61b"]))
check("plan 60's tables are untouched by the 61 additions (96 / 40 slots; lib.sh is compared with 1ecee59d below)", len(__import__("plan60").plan("60a")) == 96 and len(__import__("plan60").plan("60b")) == 40)


# ═══ 5. the engine-diff witness ═══
def git(*a):
    try:
        return subprocess.run(["git", *a], cwd=os.path.join(HERE, "..", ".."), capture_output=True, text=True, timeout=60)
    except Exception:      # noqa: BLE001
        return None


BASE = "1ecee59d"
ENGINE = ("raptorpath", "raptorpath-math", "raptorpath-core", "raptorpath-wasm", "gf256", "Cargo.toml", "Cargo.lock")
g0 = git("rev-parse", "--git-dir")
g1 = git("cat-file", "-t", BASE) if g0 is not None and g0.returncode == 0 else None
if g1 is not None and g1.returncode == 0 and g1.stdout.strip() == "commit":
    dd = git("diff", "--stat", BASE, "HEAD", "--", *ENGINE)
    check(f"engine diff against {BASE} (raptorpath, raptorpath-math, raptorpath-core, raptorpath-wasm, gf256, Cargo.toml, Cargo.lock) is EMPTY", dd is not None and dd.returncode == 0 and dd.stdout.strip() == "", dd.stdout if dd else "git failed")
    dm = git("diff", "--stat", BASE, "HEAD", "--", "tools/l1/ref60-manifest.sha256", "tools/l1/lib.sh")
    check(f"tools/l1/ref60-manifest.sha256 and lib.sh (no RWM_SLAW outside this branch's tables) equal {BASE}'s", dm is not None and dm.returncode == 0 and dm.stdout.strip() == "", dm.stdout if dm else "git failed")
    dw = git("diff", "--stat", "cb88df3a", "HEAD", "--", *ENGINE) if git("cat-file", "-t", "cb88df3a").returncode == 0 else None
    if dw is not None:
        check("section 7's witness: the engine equals s60-tree cb88df3a's", dw.returncode == 0 and dw.stdout.strip() == "", dw.stdout)
else:
    print(f"skip engine-diff witness: no git work tree or no commit {BASE} here (an archive); run test_harness61.py in the worktree")
check("the manifest the launch reuses is plan 60's (187 files)", len(rd("ref60-manifest.sha256").splitlines()) == 187)
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
