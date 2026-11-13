#!/usr/bin/env python3
"""Where does each rtp minimum sit?  Per (cell, arm, leg) over the rows of rowvals.json:
 C at start-up  = C's first minimum at t <= 0.5 s;  C at drain = C's first minimum after the last source tick (src > 0);
 A at 1st tick  = A equals the value at the leg's first in-window tick (the minimum is that tick's reading);
 A == C         = the two floors coincide;  A == first = A equals the leg's first non-zero rtp_us of the run;  A at last tick = A's first minimum is the last source-phase tick (>= last_src - 0.5 s).
usage: python -I where.py <rowvals.json> [arm,arm...]"""
import json
import sys
from collections import defaultdict

rv = json.load(open(sys.argv[1]))
arms = sys.argv[2].split(",") if len(sys.argv) > 2 else None
g = defaultdict(list)
for k, v in rv.items():
    cell, arm, seed, rep = k.split("|")
    if v.get("missing_log") or (arms and arm not in arms):
        continue
    for leg in v["C"]:
        g[(cell, arm, "p" + leg)].append((v, leg))
print("| cell | arm | leg | rows | C: start-up (t<=0.5) | C: drained tail | A defined | A: = 1st window tick | A == C | A == run's first non-zero rtp_us | A: at last source ticks (t >= last_src-0.5) |\n|---|---|---|---|---|---|---|---|---|---|---|")
for (cell, arm, leg), L in sorted(g.items()):
    n = len(L)
    su = sum(1 for v, i in L if v["C_t"][i] <= 0.5)
    dr = sum(1 for v, i in L if v["C_t"][i] > v["last_src_t"])
    ad = [(v, i) for v, i in L if i in v["A"]]
    f1 = sum(1 for v, i in ad if v["A"][i] == v["A_first"][i])
    eq = sum(1 for v, i in ad if v["A"][i] == v["C"][i])
    f0 = sum(1 for v, i in ad if v['A'][i] == v['C_first'][i])
    ls = sum(1 for v, i in ad if v["A_t"][i] >= v["last_src_t"] - 0.5)
    print("| %s | %s | %s | %d | %d | %d | %d | %d | %d | %d | %d |" % (cell, arm, leg, n, su, dr, len(ad), f1, eq, f0, ls))
