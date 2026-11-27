#!/usr/bin/env python3
"""Tracking-model replay of beta_fix = min(beta, cap), cap = room + N*xdot*L/s per window (see replay.py for the inputs).
Tracking model: B follows beta as observed now (B p95 ~ beta p95), so a quantile of B moves down by what the fix removes
from the same quantile of beta: B_fix(q) = B(q) - (beta_q(q) - min(beta_q(q), cap)), q in {p95}; B max likewise against
beta p95. E_fix = B_fix p95 - min(beta_end, cap). Reports the 68s readouts: E p50/p90/max, over-batch windows, B p95 and
B max, and the in-window spread. Usage: replay2.py <dir>"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ackclock as AC
from collections import defaultdict
q = lambda v, p: sorted(v)[int(p * (len(v) - 1))] if v else float("nan")
A = defaultdict(lambda: defaultdict(list))
for arm, rep, fn, out in AC.rows(sys.argv[1]):
    for r in out:
        if r["t"] < 2: continue
        p = r["p"]; bq = p["beta_q"]; batch = p["batch"]; beta = p["beta"]
        cap = r["room"] + r["ack"]
        cut95 = bq[2] - min(bq[2], cap)
        B95, Bmax = p["B"][1], p["B"][2]
        B95f, Bmaxf = B95 - cut95, Bmax - cut95
        bfe = min(beta, cap)
        E, Ef = B95 - beta, B95f - bfe
        a = A[f"{arm} p{r['pid']}"]
        for k, v in (("E", E), ("Ef", Ef), ("B95", B95), ("B95f", B95f), ("Bmax", Bmax), ("Bmaxf", Bmaxf), ("bmed", bq[1]), ("bmedf", min(bq[1], cap))):
            a[k].append(v)
        a["over"].append(E > batch); a["overf"].append(Ef > batch); a["big"].append(E > 50); a["bigf"].append(Ef > 50)
        a["live"].append(B95 - bq[2]); a["livef"].append(B95f - min(bq[2], cap))
for g in sorted(A):
    a = A[g]; n = len(a["E"])
    print(f"{g}: n={n} E p50/p90/max {q(a['E'],.5):.0f}/{q(a['E'],.9):.0f}/{max(a['E']):.0f} -> {q(a['Ef'],.5):.0f}/{q(a['Ef'],.9):.0f}/{max(a['Ef']):.0f}"
          f" | over-batch {sum(a['over'])} -> {sum(a['overf'])} | E>50 {sum(a['big'])} -> {sum(a['bigf'])}"
          f" | B p95 med {q(a['B95'],.5):.0f} -> {q(a['B95f'],.5):.0f}, p90 {q(a['B95'],.9):.0f} -> {q(a['B95f'],.9):.0f} | B max p99/max {q(a['Bmax'],.99):.0f}/{max(a['Bmax']):.0f} -> {q(a['Bmaxf'],.99):.0f}/{max(a['Bmaxf']):.0f}"
          f" | beta p50 med {q(a['bmed'],.5):.0f} -> {q(a['bmedf'],.5):.0f}")
