"""Stage-3 unregistered diagnostics (descriptive only; nothing here changes a reading).  python diag3.py"""
import numpy as np

import batch3
import summarize3 as S
import toy2

batch3.build()

# 1. A/A replication (phase variants 2 and 3) of the call-cell A/A
print("A/A replication, variants 2 and 3 (shown readings only):")
n = 0
for c in ("K-I2", "K-L10"):
    for k in S.AA_KEYS:
        d = S.vals(c, "AA-2", k) - S.vals(c, "AA-3", k)
        rd = S.read(d, S.T_bar(k, c))
        n += 1
        if rd["word"].startswith("shown"):
            print("  SHOWN", c, k, S.fmt(rd))
print("  %d readings checked" % n)

# 2. E-L10: age at delivery of the dead-tail units under C3 (T = inf), where a T near 1.5 s would bite
print("E-L10 and E-I2, C3: age (s) at delivery of the 347 dead-tail units, seeds 0-2")
for cell in ("E-I2", "E-L10"):
    ages = []
    for sd in range(3):
        c = batch3.cmp_cfg(cell, "C3", sd)
        sim = toy2.Sim(c)
        sim.run()
        lo = c.dead[0]
        ages += [sim.dlv[s] - sim.h_seq[s] for s in range(lo, sim.n_sent) if sim.dlv[s] >= 0]
    a = np.asarray(ages)
    print("  %s: min %.3f p50 %.3f max %.3f (n=%d)" % (cell, a.min(), np.median(a), a.max(), len(a)))

# 3. C2 in the call cells: longest single stall episode and episodes beyond 1 s
print("K cells, C2 and S2 (T=0.4, H=inf): longest stall episode (s) over seeds 0-11, and episodes of 1 s or more")
for cell in ("K-I2", "K-L10"):
    for nm, c in (("C2", lambda s: batch3.cmp_cfg(cell, "C2", s)), ("S2 H=inf", lambda s: batch3.new_cfg(cell, s, 0.4, float("inf")))):
        mx, big = [], 0
        for sd in range(12):
            sim = toy2.Sim(c(sd))
            sim.run()
            ln = [e[1] for e in sim.episodes]
            mx.append(max(ln) if ln else 0.0)
            big += sum(1 for x in ln if x >= 1.0)
        print("  %s %s: max episode mean %.3f, max over seeds %.3f, episodes >= 1 s: %d" % (cell, nm, np.mean(mx), max(mx), big))
