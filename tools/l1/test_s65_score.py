#!/usr/bin/env python3
"""Tests for status 65's registered scorer s65_score.py (docs/derivations/s65-registration.md section 5, cases C1-C8) and for the way plan 64's tables carry plan 65. The scorer itself is wired in unchanged:
its sha256 is pinned (the launch refuses another file), and this test pins it too.
Run: python3 test_s65_score.py"""
import hashlib
import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_WITNESS64", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES"):
    os.environ.pop(_k, None)
import plan64  # noqa: E402
import s65_score as s5  # noqa: E402

PIN = "41d9d58e27f42eb92862e243c25900d547ea7ffd6b64a6d00ddcb039fa844a21"
fails = 0
CHECKS = 0


def check(name, ok, detail=""):
    global fails, CHECKS
    CHECKS += 1
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def row(arm, seed, rep, inorder, status=None, dnf=False, items=None):
    st = status or ("RT-OK" if arm == "RC" else "LIVE")
    r = {"cell": "c2-100", "store": "def", "arm": arm, "seed": seed, "rep": int(rep), "status": st, "rc": 0, "dnf": dnf, "bytes": 100000000, "bin_sha": "a" * 64,
         "inorder_mean": None if dnf else inorder, "mbps": None if dnf else 70.0, "cpu_cli": None if dnf else 10.0, "cpu_srv": None if dnf else 11.0, "cli_lag_E": 1.0, "srv_lag_E": 1.0, "rtp_floor_p0": 5000,
         "qlost_2s": 600, "qsent_2s": 27000, "problems": []}
    if items:
        r["rt_items"] = items
    return r


def ledger(f=lambda a, sd, rp: None, base=lambda sd, rp: 100.0, ratio=lambda sd, rp: 1.0, n=24):
    """All n pairs of the registered table; f(arm, seed, rep) may return a replacement row or a list of rows (the redraw: first row, second row) for that slot."""
    rows = []
    for a, sd, rp in s5.plan(n):
        alt = f(a, sd, rp)
        if alt is not None:
            rows += alt if isinstance(alt, list) else [alt]
        else:
            v = base(sd, rp) * (ratio(sd, rp) if a == "RC" else 1.0)
            rows.append(row(a, sd, rp, v))
    return rows


def run(rows):
    out = []
    res = s5.score(None, out=out.append, rows=rows)
    return res, "\n".join(out)


check("s65_score.py is the registered file: sha256 41d9d58e… (the launch refuses any other)", hashlib.sha256(open(os.path.join(HERE, "s65_score.py"), "rb").read()).hexdigest() == PIN)
check("the registered constants: 24 pairs, floor 18, TOL 5 %, alpha 0.05, redraw cap 4, seeds 42 / 7", (s5.N_PAIRS, s5.MIN_PAIRS, s5.TOL, s5.ALPHA, s5.REDRAW_CAP, s5.SEEDS) == (24, 18, 0.05, 0.05, 4, ("42", "7")))
check("s65_score.plan(12) equals plan64.plan()'s slots (stage R's table), and plan 65's table is 48 rows, the first 24 of which are plan 64's layout",
      [(a, sd, str(rp)) for _, _, a, sd, rp in plan64.plan()] == s5.plan(12) and len(plan64.plan(plan_id="65")) == 48 and plan64.plan(plan_id="65")[:24] == plan64.plan() and len(s5.plan()) == 48)
check("plan 65's table in plan 64's tuple shape: (c2-100, def, arm, seed, int rep); seed 42's pair runs RB first, seed 7's RC first; its smoke is RB s42 r1, RC s42 r1 and the conditional RB s7 r1",
      plan64.plan(plan_id="65")[0] == ("c2-100", "def", "RB", "42", 1) and plan64.plan(plan_id="65")[2] == ("c2-100", "def", "RC", "7", 1)
      and plan64.plan(smoke=True, plan_id="65") == [("c2-100", "def", "RB", "42", 1), ("c2-100", "def", "RC", "42", 1), ("c2-100", "def", "RB", "7", 1)] and plan64.plan(smoke=True) == plan64.SMOKE_R)
check("plan 65's pair count and floor in plan64.py are the scorer's registered constants, its budgets are 6 700 s (launch) and 1 800 s (rehearsal)",
      (plan64.PAIRS65, plan64.MIN_PAIRS65) == (s5.N_PAIRS, s5.MIN_PAIRS) and plan64.BUDGET_S["65"] == 6700 and plan64.BUDGET_S["65r"] == 1800 and plan64.BUDGET_S["64"] == 7650)

rng = random.Random(65)
# C1 quiet null
res, txt = run(ledger(ratio=lambda sd, rp: 1.0 + (0.0 if (int(rp) + int(sd)) % 2 else 0.0)))
check("C1 a quiet null (RC == RB): NOT-REPLICATED, U printed", res["label"] == "NOT-REPLICATED" and "one-sided 95 % upper bound U=" in txt and "VERDICT65 NOT-REPLICATED" in txt, txt[-600:])
# C2 +25 %
noise = {(sd, str(rp)): math.exp(rng.gauss(0, 0.05)) for sd in s5.SEEDS for rp in range(1, 13)}
res, txt = run(ledger(ratio=lambda sd, rp: 1.25 * noise[(sd, rp)]))
check("C2 +25 % (small noise): REPLICATED", res["label"] == "REPLICATED" and res["test"]["h_rel"] > 0.2 and res["test"]["p"] <= 0.05, txt[-500:])
# C3 a resolved +3 %
res, txt = run(ledger(ratio=lambda sd, rp: 1.03 * noise[(sd, rp)]))
check("C3 a resolved +3 %: NOT-REPLICATED (the shift is below TOL even though the test may resolve it)", res["label"] == "NOT-REPLICATED" and res["test"]["h_rel"] < 0.05, (res["label"], res["test"]))
# C4 noisy, no shift
noisy = {(sd, str(rp)): math.exp(rng.gauss(0, 0.5)) for sd in s5.SEEDS for rp in range(1, 13)}
res, txt = run(ledger(ratio=lambda sd, rp: noisy[(sd, rp)]))
check("C4 pairs as noisy as section 64's (+-50 %) and no shift: NOT-REPLICATED, with a large finite U printed (the closing statement carries its uncertainty)",
      res["label"] == "NOT-REPLICATED" and res["test"]["u_rel"] != math.inf and res["test"]["u_rel"] > 0.05, res["test"])
# C5 seven slots dead twice -> 17 pairs
dead_slots = {("42", "1"), ("42", "2"), ("42", "3"), ("7", "1"), ("7", "2"), ("7", "3"), ("7", "4")}
res, txt = run(ledger(lambda a, sd, rp: [row(a, sd, rp, 0, status="BASE-F2-DEAD", dnf=True), row(a, sd, rp, 0, status="BASE-F2-DEAD", dnf=True)] if (a == "RB" and (sd, rp) in dead_slots) else None, ratio=lambda sd, rp: 1.25 * noise[(sd, rp)]))
check("C5 seven slots dead twice (17 usable pairs): UNRESOLVED-N, the dead pairs not counted, dead twice 7", res["label"] == "UNRESOLVED-N" and res["n"] == 17 and "dead twice 7" in txt, txt[:900])
# C6 a dead RB redrawn alive
res, txt = run(ledger(lambda a, sd, rp: [row(a, sd, rp, 0, status="BASE-F2-DEAD", dnf=True), row(a, sd, rp, 100.0)] if (a == "RB" and (sd, rp) == ("42", "1")) else None, ratio=lambda sd, rp: 1.25 * noise[(sd, rp)]))
check("C6 a dead RB row redrawn alive: the slot is scored (24 usable pairs), the redraw is counted", res["n"] == 24 and "redrawn alive 1" in txt and "REDRAWS BASE-F2-DEAD 1" in txt, txt[:700])
# C7 an RC RT-FAIL
res, txt = run(ledger(lambda a, sd, rp: row(a, sd, rp, 100.0, status="RT-FAIL", items=[2]) if (a == "RC" and (sd, rp) == ("7", "5")) else None, ratio=lambda sd, rp: 1.25 * noise[(sd, rp)]))
check("C7 an RC RT-FAIL: F2-FINDING65 printed (to the owner), the pair is dropped (23 usable), the item is named", res["f2"] == 1 and res["n"] == 23 and "F2-FINDING65 1 RC row(s) RT-FAIL: s7 r5 items [2]" in txt, txt[:900])
# C8 an RC DNF
res, txt = run(ledger(lambda a, sd, rp: row(a, sd, rp, 0, dnf=True) if (a == "RC" and (sd, rp) == ("42", "2")) else None, ratio=lambda sd, rp: 1.25 * noise[(sd, rp)]))
check("C8 an RC DNF is the worst rank on its side: d = +L (L = 1 + max |finite d|), the pair is scored", res["n"] == 24 and "s42r2:+1." in txt.replace("(rc-dnf)", "") and "(rc-dnf)" in txt, txt[:1100])
# an RB DNF (live RB row that did not complete) is d = -L; both DNF is 0
res, txt = run(ledger(lambda a, sd, rp: row(a, sd, rp, 0, dnf=True) if (a in ("RB", "RC") and (sd, rp) == ("42", "2")) else (row(a, sd, rp, 0, dnf=True) if (a == "RB" and (sd, rp) == ("7", "2")) else None), ratio=lambda sd, rp: 1.25 * noise[(sd, rp)]))
check("both rows of a slot DNF: d = 0 ('both-dnf'); an RB DNF alone: d = -L ('rb-dnf')", "(both-dnf)" in txt and "(rb-dnf)" in txt and "s7r2:-" in txt, txt[:1200])
# redraw cap: the scorer only takes a slot's second row; the cap is the battery's (plan64.replace), tested below
import io  # noqa: E402
import json  # noqa: E402
import shutil  # noqa: E402
import tempfile  # noqa: E402
from contextlib import redirect_stdout  # noqa: E402


def rep_ledger(rows):
    d = tempfile.mkdtemp()
    p = os.path.join(d, "ld.log")
    with open(p, "w", newline="\n") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r, sort_keys=True) + "\n")
    return d, p


def qrep(p, a, sd, rp, plan_id):
    with redirect_stdout(io.StringIO()):
        return plan64.replace(p, "c2-100", a, sd, rp, plan_id)


dead = row("RB", "42", "1", 0, status="BASE-F2-DEAD", dnf=True)
d, p = rep_ledger([dead])
check("plan64.replace under plan 65: a BASE-F2-DEAD RB row is redrawn (once); under plan 64 it never is (unchanged)", qrep(p, "RB", "42", "1", "65") == 0 and qrep(p, "RB", "42", "1", "64") == 1 and qrep(p, "RB", "42", "1", "64") == 1)
shutil.rmtree(d, ignore_errors=True)
d, p = rep_ledger([row("RC", "42", "1", 100, status="RT-FAIL", items=[2]), row("RC", "7", "1", 100, status="F2-FAIL")])
check("plan64.replace under plan 65: an RC F2 failure is never redrawn", qrep(p, "RC", "42", "1", "65") == 1 and qrep(p, "RC", "7", "1", "65") == 1)
shutil.rmtree(d, ignore_errors=True)
d, p = rep_ledger([row("RB", "42", "1", 0, status="VOID-RC"), row("RB", "42", "1", 100.0)] + [row("RB", "7", str(k), 0, status="BASE-F2-DEAD", dnf=True) for k in (1, 2, 3)]
                  + [row("RB", "42", "2", 0, status="BASE-F2-DEAD", dnf=True)])
# slot (42,1) holds two rows already: 1 slot redrawn so far; the cap is 4 slots
check("plan64.replace under plan 65: the cap is 4 slots (s65_score.REDRAW_CAP): with 1 slot already redrawn a dead row is still redrawn, with 4 it is not; under plan 64 the cap stays 2",
      qrep(p, "RB", "7", "1", "65") == 0 and qrep(p, "RB", "42", "1", "65") == 1, None)
more = [row("RB", sd, rp, 0, status="BASE-F2-DEAD", dnf=True) for sd, rp in (("42", "3"),)] + [row("RB", sd, rp, 100.0) for sd, rp in (("42", "3"), ("7", "2"), ("7", "3"))]
shutil.rmtree(d, ignore_errors=True)
rows4 = []
for k in (1, 2, 3, 4):
    rows4 += [row("RB", "42", str(k), 0, status="BASE-F2-DEAD", dnf=True), row("RB", "42", str(k), 100.0)]
rows4.append(row("RB", "7", "1", 0, status="BASE-F2-DEAD", dnf=True))
d, p = rep_ledger(rows4)
check("plan64.replace under plan 65: with four slots already redrawn the fifth dead row is NOT redrawn (cap 4); under plan 64 a cap of 2 had stopped it earlier", qrep(p, "RB", "7", "1", "65") == 1 and plan64.MAX_REPLACEMENTS_R == 2 and s5.REDRAW_CAP == 4)
shutil.rmtree(d, ignore_errors=True)
# the conditional smoke row
d, p = rep_ledger([dead])
c1 = plan64.cond65(p)
shutil.rmtree(d, ignore_errors=True)
d, p = rep_ledger([row("RB", "42", "1", 100.0)])
c2 = plan64.cond65(p)
shutil.rmtree(d, ignore_errors=True)
check("plan64.cond65: the conditional third smoke row runs iff RB s42 r1 is BASE-F2-DEAD", c1 == 0 and c2 == 1)
print("test_s65_score: %d checks, %d failed" % (CHECKS, fails))
sys.exit(1 if fails else 0)
