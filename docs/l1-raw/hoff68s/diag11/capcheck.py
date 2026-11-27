#!/usr/bin/env python3
"""Does cap = room + N*xdot*L/s bind (end samples) on GBU and in Startup windows of the Bulk rows? Usage: capcheck.py <dir>"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J, series as SE
N = 1.25
for arm, rep, fn in J.rows(sys.argv[1]):
    if arm not in ("GBU", "GK2", "GA2", "GK8", "GPH"): continue
    Q = SE.qcc(fn); ws = [w for w in J.windows(fn) if w[1] == "on"]
    bind = n = 0; worst = 9.0; modes = {}
    for j in range(1, len(ws)):
        t, t0 = ws[j][0], ws[j-1][0]; a = Q.get((t, 0)); b = Q.get((t0, 0)); p = ws[j][2].get(0)
        if not a or not b or not p or not p.get("R") or not p.get("beta"): continue
        if arm != "GBU" and a["qm"] != "S": continue
        s = p["R"] * p["L"] / 1e3 / p["beta"]; xd = (int(a["qab"]) - int(b["qab"])) / (t - t0)
        cap = max(p["win"] - p["F"], 0) / s + N * xd * p["L"] / 1e3 / s
        n += 1; r = cap / p["beta"]; worst = min(worst, r); bind += r < 1
        modes[a["qm"]] = modes.get(a["qm"], 0) + 1
    print(arm, rep, "windows", n, "modes", modes, "cap<beta(end) in", bind, "min cap/beta=%.2f" % worst)
