#!/usr/bin/env python3
"""Within-window beta spread split by whether L moved between the bracketing end samples. Usage: lsplit.py <dir>"""
import sys, os, statistics as st
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J
from collections import defaultdict
agg = defaultdict(lambda: defaultdict(list))
for arm, rep, fn in J.rows(sys.argv[1]):
    if arm[0] != "G" or arm == "GBU": continue
    ws = [w for w in J.windows(fn) if w[1] == "on"]
    for j in range(1, len(ws)):
        if ws[j][0] < 2: continue
        for pid in (0, 1):
            if pid not in ws[j][2] or pid not in ws[j-1][2]: continue
            c = J.classify(ws, j, pid)
            if not c: continue
            L0 = ws[j-1][2].get(pid, {}).get("L"); L1 = c["L"]
            p, pp = ws[j][2][pid], ws[j-1][2][pid]
            r = c["bq95"] / c["bq50"]
            same = abs(L1 / L0 - 1) < 0.02 if L0 else None
            exc = max(c["E"] - c["batch"], 0)
            g = f"{arm[:3]} p{pid}"
            agg[g]["same" if same else "moved"].append((r, exc))
for g in sorted(agg):
    out = []
    tot = sum(e for k in agg[g] for _, e in agg[g][k])
    for k in ("same", "moved"):
        v = agg[g][k]
        if not v: continue
        rs = sorted(x for x, _ in v)
        out.append(f"L {k}: n={len(v)} ratio p50={rs[len(rs)//2]:.2f} p90={rs[int(.9*len(rs))]:.2f} excess share={sum(e for _, e in v)/max(tot,1):.2f}")
    print(g, " | ".join(out))
