#!/usr/bin/env python3
"""Does B follow beta? Per window, B p50 - beta p50 and B p95 - beta p95 (t >= 2 s, GATE Bulk rows). Usage: track.py <dir>"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J
from collections import defaultdict
q = lambda v, p: sorted(v)[int(p * (len(v) - 1))]
A = defaultdict(lambda: defaultdict(list))
for arm, rep, fn in J.rows(sys.argv[1]):
    if arm[0] != "G" or arm == "GBU": continue
    for t, g, P in J.windows(fn):
        if g != "on" or t < 2: continue
        for pid, p in P.items():
            if not p.get("beta_q") or not p.get("B"): continue
            a = A[f"{arm} p{pid}"]
            a["d50"].append(p["B"][0] - p["beta_q"][1]); a["d95"].append(p["B"][1] - p["beta_q"][2])
for g in sorted(A):
    a = A[g]
    print(f"{g}: B p50 - beta p50 p10/50/90 = {q(a['d50'],.1):.0f}/{q(a['d50'],.5):.0f}/{q(a['d50'],.9):.0f} | B p95 - beta p95 p10/50/90 = {q(a['d95'],.1):.0f}/{q(a['d95'],.5):.0f}/{q(a['d95'],.9):.0f}")
