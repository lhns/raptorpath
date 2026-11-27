#!/usr/bin/env python3
"""68s ex_max (same-instant B - live beta at a turn end) over over-batch windows; split, and the part in windows the cap would cut. Usage: exmax.py <dir>"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ackclock as AC
from collections import defaultdict
q = lambda v, p: sorted(v)[int(p * (len(v) - 1))] if v else float("nan")
A = defaultdict(lambda: defaultdict(list))
for arm, rep, fn, out in AC.rows(sys.argv[1]):
    for r in out:
        if r["t"] < 2: continue
        p = r["p"]; ex = p.get("ex_max")
        if not ex or p["B"][1] - p["beta"] <= p["batch"]: continue
        cap = r["room"] + r["ack"]; cut = p["beta_q"][2] > cap
        a = A[f"{arm} p{r['pid']}"]
        a["tot"].append(ex[0]); a["fs"].append(ex[1]); a["exm"].append(ex[2]); a["cut"].append(cut)
        if cut: a["tot_cut"].append(ex[0]); a["fs_cut"].append(ex[1])
for g in sorted(A):
    a = A[g]
    print(f"{g}: over-batch n={len(a['tot'])} ex_max total p50/p90/max={q(a['tot'],.5):.0f}/{q(a['tot'],.9):.0f}/{max(a['tot']):.0f} booked first={sum(a['fs']):.0f} exempt={sum(a['exm']):.0f} total={sum(a['tot']):.0f}"
          f" | windows the cap cuts: {sum(a['cut'])}, their ex_max total={sum(a['tot_cut']):.0f} first={sum(a['fs_cut']):.0f}")
