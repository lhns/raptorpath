#!/usr/bin/env python3
"""Step sizes between consecutive end samples: L, tau_p, win/srtt (= R/1.25). Usage: steps.py <dir>"""
import re, sys, os, math
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J, recjoin as RJ
from collections import defaultdict
A = defaultdict(lambda: defaultdict(list))
for arm, rep, fn in J.rows(sys.argv[1]):
    if arm[0] != "G" or arm == "GBU": continue
    T = RJ.hoff_terms(fn)
    ws = [w for w in J.windows(fn) if w[1] == "on" and w[0] >= 2]
    for j in range(1, len(ws)):
        for pid in (0, 1):
            a, b = ws[j-1][2].get(pid), ws[j][2].get(pid)
            if not a or not b or not a.get("R") or not b.get("R"): continue
            g = f"{arm} p{pid}"
            A[g]["dL"].append(abs(math.log(b["L"] / a["L"])))
            A[g]["dR"].append(abs(math.log(b["R"] / a["R"])))
            A[g]["L"].append(b["L"]); A[g]["tp"].append(T[(ws[j][0], pid)]["tau_p"]); A[g]["to"].append(T[(ws[j][0], pid)]["tau_o"])
q = lambda v, p: sorted(v)[int(p * (len(v) - 1))]
for g in sorted(A):
    a = A[g]
    print(f"{g}: |dlogL| p50/p90/max={q(a['dL'],.5):.3f}/{q(a['dL'],.9):.3f}/{max(a['dL']):.3f}  |dlogR| p50/p90/max={q(a['dR'],.5):.3f}/{q(a['dR'],.9):.3f}/{max(a['dR']):.3f}"
          f"  L p5/50/95={q(a['L'],.05):.2f}/{q(a['L'],.5):.2f}/{q(a['L'],.95):.2f}  tau_p p5/50/95={q(a['tp'],.05):.2f}/{q(a['tp'],.5):.2f}/{q(a['tp'],.95):.2f}  tau_o values={sorted(set(a['to']))[:6]}")
