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

# 4. K-L10 round sweep, resends by origin (sweep fire or report-driven), seed 0, H = 0
print("K-L10 resends by origin (seed 0, H = 0): copies sent exactly at a sweep fire instant / at a report arrival")
for T in (0.14, 0.16, 0.17, 0.18, 0.20, 0.22):
    sim = toy2.Sim(batch3.new_cfg("K-L10", 0, T, 0.0))
    sim.cfg.want_trace = True
    sim.trace = []
    r = sim.run()
    P, ph = sim.sweep_P, sim.ph_w
    nsw = nrep = 0
    ages_sw, ages_rep = [], []
    for (t, s, k, _, kind) in sim.trace:
        if kind != 1:
            continue
        x = (t - ph) / P
        if abs(x - round(x)) * P < 1e-6:
            nsw += 1
            ages_sw.append(t - sim.h_seq[s])
        else:
            nrep += 1
            ages_rep.append(t - sim.h_seq[s])
    print("  T=%.2f: sweep %d (median age %.3f), report %d (median age %.3f), share_T %.4f" % (
        T, nsw, np.median(ages_sw) if ages_sw else float("nan"), nrep, np.median(ages_rep) if ages_rep else float("nan"), r["share_T"]))

# 5. floors per unit handed over and per delivered, K-L10 at T = 0.10 and 0.14
print("K-L10 floors: per delivered and per unit handed over (means over 12 seeds)")
for T in (0.10, 0.14):
    nm = "SW-%s" % S.tag(T)
    fl = [S.R("K-L10", nm, s)["gauges"]["floors"] for s in range(12)]
    print("  T=%.2f: floors %.1f per run, per delivered %.4f, per unit %.4f" % (T, np.mean(fl), S.mean("K-L10", nm, "floors_per_delivered"), np.mean(fl) / 3000))

# 6. A/A repair residuals in the call cells, per seed
print("A/A repair_per_delivered per seed (variant 0 minus variant 1)")
for c in ("K-I2", "K-L10"):
    d = S.vals(c, "AA-0", "repair_per_delivered") - S.vals(c, "AA-1", "repair_per_delivered")
    print("  %s: %s" % (c, " ".join("%.3f" % x for x in d)))
