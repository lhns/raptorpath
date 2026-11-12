# POST HOC. Per-cell BASE vs CAND medians for every feature; rank by |rel delta| at c8.
import json, sys, statistics as st
R = json.load(open(sys.argv[1])); CELLS = ["c1s-400", "c1d-400", "c2-100", "c8-100"]
pat = sys.argv[2] if len(sys.argv) > 2 else ""
keys = sorted({k for r in R for k, v in r.items() if isinstance(v, (int, float))})
def med(cell, arm, k):
    v = [r[k] for r in R if r["cell"] == cell and r["arm"] == arm and isinstance(r.get(k), (int, float))]
    return st.median(v) if len(v) >= 6 else None
res = []
for k in keys:
    if pat and pat not in k: continue
    row = []
    for c in CELLS:
        b, a = med(c, "BASE", k), med(c, "CAND", k)
        row.append((b, a, (a / b - 1) * 100 if b and a is not None and b != 0 else None))
    res.append((k, row))
res.sort(key=lambda x: -abs(x[1][3][2] or 0))
print(f"{'key':40s} " + " ".join(f"{c:>26s}" for c in CELLS))
for k, row in res:
    print(f"{k[:40]:40s} " + " ".join(f"{(b or 0):10.4g}{(d if d is not None else float('nan')):+7.1f}%" + " " * 8 for b, a, d in row))
